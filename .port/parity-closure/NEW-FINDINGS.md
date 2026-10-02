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
- **state**: **STANDING (not a defect)** (triage T-1, 2026-09-28): a test-helper diagnostic (`testViewer`, test code only) that is unportable by signature; the controller ruling of 2026-09-09 above accepts it as permanently unportable, the PORT NOTE stands at `crates/rustty-vt/src/tmux/viewer.rs:2929`, and G-LOGFILE's test-only rule (task tail-s1b, `00e68940`) no longer counts the site (`tmux/viewer.zig` deficit 0). Was: **OPEN — will not fix.** Closing it requires changing
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
- **state**: **CLOSED** (G-1, 2026-09-28). Each C1 site has its own table: `SEEN_UTF8_DECODED_C1_SLICE` (the `nextSlice` decode loop, `stream.zig:734`, `u32` args) and `SEEN_UTF8_DECODED_C1_CODEPOINT` (`handleCodepoint`, `:1185`, `u21` args); the KNOWN DIVERGENCE note is replaced by the pairing rule. Test `stream::tests::utf8_decoded_c1_sites_have_separate_once_tables` drives the slice path and the per-byte path in one process and asserts each static claims its slot (each can hold at most one, key 0x80, so the test is order-independent). Negative control: the codepoint site pointed back at the slice table fails "the per-codepoint path's C1 arm must claim its own dedup slot" (left 1, right 0). Was: **OPEN** (triage T-1, 2026-09-28). Re-verified at the pin: the two sites pass `cp: u32` (`stream.zig:734`) and `c: u21` (`:1185`), two `logUnsupportedOnce` instantiations with two `Static.seen` tables; the port still shares `SEEN_UTF8_DECODED_C1` (`stream.rs:1181`, `:1886`, `:4800`). Left: one static per site plus a test that drives both sites in one process and asserts two records. Effort S, no long run; observable only as one missing warning line per process (diagnostic text, R1). Was: **OPEN** — to be fixed with a test that drives both call sites in
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
- **state**: **CLOSED** (triage T-1, 2026-09-28): the swap is done. Task T-F `c539d380` ("the CoreText shaper runs the real CFReleaseThread", closes S94-F1/S94-F2) and T-F2 `09bdf1b2` (a recording CFAllocator proves the pool is freed on the release thread); `S94-F1` is CLOSED in `LEDGER.tsv`; see N24's discharge. Was: **OPEN — needs an owner.** The swap is a runtime refactor: replace the mpsc
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

## N7 — `metal/buffer.zig`'s two null-contents warns are unrestorable and unclaimed

- **id**: N7
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

---

## N8 — `initPostPipelines`'s error set collapsed to one variant, so its restored log always prints the same thing

- **id**: N8
- **severity**: low (a diagnostic that is present and correctly placed but permanently
  under-informative; no state or rendering difference)
- **kind**: error-type degeneration / behavioral-difference
- **oracle**: `src/renderer/metal/shaders.zig:151` —
  `log.warn("error initializing postprocess shaders err={}", .{err});` in the
  `initPostPipelines(...) catch |err| err: { ... break :err &.{}; }` arm. `err` there
  ranges over the inferred error union of `initPostPipelines`, which spans allocation
  failure, `error.MetalFailed` from either `checkError` site, and the shader-source
  decode path — so Zig's `{}` renders a DIFFERENT `error.<Name>` per cause.
- **rustty**: `crates/rustty-renderer/src/metal/shaders.rs` — `init_post_pipelines`
  returns `Result<Vec<Pipeline>, Error>` where `Error` has exactly one variant,
  `MetalFailed`. Every failure in that call tree is flattened into it (four separate
  `.map_err(|_| Error::MetalFailed)` / `.map_err(|_nserr| ...)` sites feed it).
- **description**: the restored emission is at the right place, at the right level,
  with the reference's verbatim message text — but its one argument can only ever
  render `MetalFailed`, so the diagnostic cannot distinguish an out-of-memory from a
  spirv-cross-produced MSL that failed to decode as UTF-8 from a Metal compile error.
  The reference distinguishes all three. This is an ERROR-TYPE divergence, not a
  logging one: it predates the log restoration and would exist identically if the log
  had never been dropped.
- **how found**: task 14c-2, restoring `S119-F1`'s three sites. It is the shape the
  brief warned about ("a function's error type had degenerated so the restored message
  would print `err=()`") in a milder form — degenerate to one NAMED variant rather than
  to `()`, which is why it survives a casual read of the restored line.
- **why this is recorded separately from S119-F1**: `S119-F1` is legitimately CLOSED —
  all three of its cited anchors are live emissions with verbatim text, which is what
  the row asks for. Widening a Metal error set is a runtime refactor and must not ride
  along inside a `LOG`-class row's closure (the same principle applied to [[N5]]). But
  the divergence is knowable and permanent, and right now it lives only in a row
  disposition inside a CLOSED row, where the finale re-audit would not meet it as a
  tracked item.
- **state**: **CLOSED** (G-1, 2026-09-28). `metal::shaders::Error` is the reference set of this path: `MetalFailed` (`checkError`) and `OutOfMemory`, the name `macos.foundation.String.createWithBytes` gives a failed `CFStringCreateWithBytes` (`pkg/macos/foundation/string.zig:8-20`), which is what a custom shader whose bytes are not UTF-8 hits; the port's `from_utf8` arm now maps to it (it was `MetalFailed`). The enum renders `error.<Name>` through `Display` and the `shaders.zig:151` site prints `err={err}`; `generic.rs`'s `From<shaders::Error>` keeps the variant (`OutOfMemory` → `Error::OutOfMemory`). Pipeline.init's own causes are all `MetalFailed` in the reference too. Tests (Metal, `RUSTTY_REQUIRE_METAL=1`): `metal::shaders::post_pipeline_error_tests::metal_compile_error_logs_error_metal_failed` and `::non_utf8_source_logs_error_out_of_memory` assert the full warn vector. Negative controls: the decode arm back to `MetalFailed` fails the non-UTF-8 test (right `err=error.MetalFailed`); the site back to `{err:?}` fails both (`err=MetalFailed`). Was: **OPEN** (triage T-1, 2026-09-28). Unchanged: `metal::shaders::Error` still has the one variant `MetalFailed` (`shaders.rs:49-52`) and the site still prints `err={err:?}` (`:394`). Left: variants for the reference's reachable causes (the `createWithBytes` failure and the Metal compile error; OutOfMemory aborts in Rust) threaded through the flattening `map_err`s, rendered as `error.<Name>` (joins N11). Effort S, no long run; observable only in one warn line's text (R1). Was: **OPEN — needs an owner.** Closing it means giving
  `metal::shaders::Error` (and the `metal::pipeline::Error` it folds in) variants that
  distinguish the reference's causes — at minimum allocation/decode/Metal-compile —
  and threading them through the four flattening `map_err`s. Note the same flattening
  is what makes every other `{err:?}` site in this crate unverifiable against the
  oracle's rendering (task-14c-1 fix round, concern 1); this row is simply the one
  case where the collapse is total and therefore provable without a ruling.

---

## N9 — the Linux twins of `generic.zig` and `image.zig` were logging-dead while their macOS twins were live (CLOSED by task-14c-5)

- **id**: N9
- **severity**: medium (a Linux/FreeBSD build is silent on ~30 diagnostics the reference
  emits and the macOS build now emits; no state or rendering difference)
- **kind**: uncited-site / twin drift
- **oracle**: `src/renderer/generic.zig` (the `rebuildCells`/`rebuildRow`/preedit/cursor/
  overlay/background-image `log.warn` family) and `src/renderer/image.zig` (the kitty
  placement / upload family).
- **rustty**: `crates/rustty-renderer/src/generic/linux_impl.rs` and
  `crates/rustty-renderer/src/image/linux_impl.rs`.
- **description**: stage 14c restored these call sites in the **macOS** twins only.
  Measured at the end of task-14c-3:

  | file | live `log::` emissions | remaining commented reference calls |
  |---|---|---|
  | `generic.rs` (macOS twin) | 31 | 44 |
  | `generic/linux_impl.rs` | **1** | 16 |
  | `image/macos_impl.rs` | 11 | 20 |
  | `image/linux_impl.rs` | **0** | 19 |

  (The "commented" column counts comment ANCHORS, not sites — a multi-line block comment
  reproducing one reference call and its one-line stub both match, so it roughly
  double-counts. PROCEDURE.md blind spot 3.) The macOS numbers are the correct ones; the
  Linux twins are the same reference calls, not ported.
- **how found**: task-14c-3, enumerating the oracle's own `log.*` calls across the
  Linux-only files it was in (PROCEDURE.md blind spot 3). The stage restored the OpenGL
  ones it owned (`OpenGL.zig`, `opengl/Frame.zig`, `opengl/shaders.zig`, and the
  `pkg/opengl` logger's routing) and deliberately did NOT take on the two big twins —
  they are a different unit of work, they are Linux-only (no local typecheck), and one of
  them (`image/linux_impl.rs`) already has an OPEN row of its own, `S117-F2`.
- **affected rows**: exactly three — **`S115-F4`**, **`S116-F2`** (both
  `src/renderer/generic.zig`, task-14c-4) and **`S117-F3`** (`src/renderer/image.zig`,
  task-14c-2). All three are `WAIVED` with `owner=.port/parity-closure/ADJUDICATION.md#…`,
  and their adjudication text now says explicitly that the restoration is macOS-twin only
  and points here.
  *(Corrected in fix round 1. The first version of this entry named `S113-F2`, `S117-F3`
  and `S119-F1`. `S113-F2` (`renderer/Metal.zig`) and `S119-F1` (`renderer/metal/
  shaders.zig`) are Metal-ORACLE rows: there is no Linux twin of those oracle files at
  all, so they are unaffected. `S115-F4` and `S116-F2` were omitted and are affected —
  their oracle IS `generic.zig`, whose Linux twin is `generic/linux_impl.rs`.)*
- **why this is recorded separately**: those three rows name only the macOS twin in their
  `rustty` column, so their `WAIVED` state does not rest on these sites and the
  blind-spot-1 carve-out is not in play. But a reader of those rows would reasonably
  believe the diagnostics are restored, and on Linux they are not. This entry is so the
  finale re-audit meets that as a tracked item.
- **size**: **35 genuinely unported call sites** across the two files (24 in
  `generic.zig`, 11 in `image.zig`), producing 36 emissions. The figure first written here
  was "roughly 43", from the 14c-3 fix round; it was wrong, and it is corrected in place
  rather than only below, because a reader greps the size bullet. The 43 counted six
  `generic.zig` sites (`:991`, `:1169`, `:1177`, `:1183`, `:1188`, `:1190`) that sit behind
  `if (comptime DisplayLink == void) return;` and therefore have **no Linux counterpart to
  port at all**; the Linux twin has no display-link field. See the state block below and
  task-14c-5 §2.1. That is a task, not a fix round; narrowing the three dispositions was
  the honest interim state.
- **state**: **CLOSED (task-14c-5, 2026-09-10).** Both Linux twins are restored.
  `generic/linux_impl.rs` carries `const LOG_SCOPE: &str = "generic_renderer";` at file
  scope and 25 live emissions covering 24 `generic.zig` call sites; `image/linux_impl.rs`
  carries `const LOG_SCOPE: &str = "renderer_image";` and 11 live emissions. Verified on
  the Linux VM (build / test / clippy `-D warnings` / workspace `fmt --check`, plus the
  test pass under `RUSTTY_REQUIRE_GL=1`).

  Corrections to this entry's own numbers, found by enumerating the reference rather
  than trusting them:

  - **"~43 genuinely unported call sites" was high.** The real figure is **35**
    restorable sites (24 in `generic.zig`'s Linux arm + 11 in `image.zig`'s), and the
    35 emissions are 36 because `generic.zig:1974` needs two arms (the io error and
    the reference's `error.StreamTooLong` 4 GiB cap).
  - **Six `generic.zig` sites have NO Linux counterpart at all** and were never
    "unported": `:991` (`displayLinkCallback`) and `:1169/:1177/:1183/:1188/:1190`
    (`syncDisplayLink`) are guarded by `if (comptime DisplayLink == void) return;`,
    and `DisplayLink` is `void` off macOS. Counting them as a Linux gap is what
    inflated the estimate.
  - **The "1 live emission" in `generic/linux_impl.rs` was not a `generic.zig` site.**
    It is `opengl/Frame.zig:65`'s present-failure, which task-14c-3 landed in that file
    because the port inlines `Frame.zig`'s completion tail into
    `opengl_frame_completed`. Against `generic.zig` the Linux twin stood at **zero**,
    not one — the same "a reference call lands in a different file than its name
    suggests" shape 14c-1 and 14c-2 each hit once.

  **What remains — nothing that is not already a WAIVED row's own residual.** Five
  reference sites are unrestorable on Linux, and they are the SAME five that are
  unrestorable on macOS, for the same reason (the Rust operation is infallible, so
  there is no failure arm and hoisting the warn would put a diagnostic on the SUCCESS
  path): `generic.zig:1558` (S115-F4), `:2912` and `:3181` (S116-F2), `image.zig:365`
  and `:447` (S117-F3). Each carries a PORT NOTE at its Linux code site and is recorded
  as a Linux-side residual in the row's `ADJUDICATION.md` entry. The three rows stay
  **WAIVED** — this task closed no rows and changed no row state.

  The `eprintln!` stand-in class showed up again: `generic/linux_impl.rs`'s
  `prep_background_image` had three of them AND the same collapsed `std::fs::read`
  that task-14c-1 had to split in the Metal twin (the reference logs a different
  message for open and for read). Both are fixed.
- **why the two twins really are mutually exclusive** (checked in fix round 1, because it
  was disputed and the disputing read stopped one level too early): `generic.rs` is not
  itself platform-gated, but all 32 of its `log::` emissions sit inside its
  `#[cfg(target_os = "macos")] mod macos_impl` (which begins at `generic.rs:1095`), and
  `generic/linux_impl.rs` is reached only through
  `#[cfg(any(target_os = "linux", target_os = "freebsd"))]`. Never both in one build.

---

## N10 — `web-canvas-instantiate.mjs` was dead for ~1 day: this plan's own log backend added an `env.log` import the glue did not supply

- **id**: N10
- **severity**: low-medium (no shipped artifact affected — the break is in a verification
  script, not in `rustty_web_canvas.wasm` itself. Medium only because the dead script is
  the sole headless check for the whole freestanding web-canvas deliverable, so while it
  was dead nothing was checking `W9F-03`/`W9F-04` or the "0 wasi imports" link invariant.)
- **kind**: tooling regression introduced by this plan's own work (not a port defect, not
  an oracle divergence)
- **oracle**: none — `.port/tools/*.mjs` are rustty-only harnesses with no ghostty analogue.
- **rustty**: `.port/tools/web-canvas-glue.mjs` (`makeWebCanvasEnv`) broke
  `.port/tools/web-canvas-instantiate.mjs`.
- **description**: `install_wasm_log_backend()` registers `rustty-os`'s `WasmLogger`, which
  makes `rustty_web_canvas.wasm` import `env.log` — in BOTH profiles (release strips the
  restored call-site bodies but keeps the import). `makeWebCanvasEnv` supplied only the
  fourteen `rustty_wc_*` Canvas ops, and a missing wasm import is a hard `LinkError`, so
  from the moment the backend landed:

  ```
  $ node .port/tools/web-canvas-instantiate.mjs
  LinkError: WebAssembly.instantiate(): Import #14 "env" "log": function import requires a callable
  ```

  Every one of that script's checks — 0 wasi imports, instantiation, face/atlas/shaper
  callability, the `(c4)` DeferredFace arms, the `(c5)` BGRA byte-order assertion, the
  `(c6)` font-file-bytes rejection — was unreachable behind that throw.
- **broken by**: `9a41c0c2` ("task-14c-4 (part 4): give rustty_web_canvas.wasm a log
  backend"), which put the first `install_wasm_log_backend()` calls into
  `rustty-font-shaper`'s seven exports and therefore introduced the `env.log` import.
  `3a90678c` (the artifact-wide fix) widened the installer to the other twelve exports but
  did not cause this; the import already existed.
- **closed by**: `2ca1a947` (task-14wasm-proof). `makeWebCanvasEnv` now takes an optional
  `log` and supplies a discarding default, so every existing caller works unchanged and the
  new `.port/tools/wasm-log-emission.mjs` passes a capturing one. `node
  .port/tools/web-canvas-instantiate.mjs` → ALL OK again.
- **how found**: task-14wasm-proof, while reusing `web-canvas-glue.mjs` to build the
  log-emission harness. Running the sibling script once, to confirm the glue still worked,
  is what surfaced it — it was not found by any gate, test, or review.
- **why this is recorded**: **no gate runs `web-canvas-instantiate.mjs`.** It is referenced
  by two mutation-proven `.port/phase-2/DEFERRALS.md` entries (`W9F-03`, `W9F-04`) as their
  standing evidence, and by `.port/tools/CI.md` nowhere. That is exactly why the rot went
  unnoticed: a harness with no gate is a harness whose failure nobody observes. The same
  argument is why `wasm-log-emission.mjs` was wired into `ci.sh`'s opt-in `--webgl` lane
  rather than left as a documented command.
- **state**: **CLOSED** for the glue defect itself.

  **RESIDUAL, OPEN, and deliberately left for a future worker:**
  `web-canvas-instantiate.mjs` is *still ungated*, so the exact rot mechanism demonstrated
  here remains open for that script — the next import the artifact grows will kill it again,
  silently. The fix is three lines in `run_webgl_lane` (`.port/tools/ci.sh`), beside the
  log-emission gate that now runs there, using the RELEASE artifact that lane already
  builds. Judged out of scope for task-14wasm-proof by its reviewer; recorded here so
  whoever next touches that lane trips over it.

---

## N11 — restored `log` sites render Rust's derived formatter where Zig renders its builtin aggregate formatter, crate-wide across ~35 sites

- **id**: N11 (next free id checked at the time of writing: N1-N10 taken, N10 by another
  worker — ids have collided twice on this branch)
- **severity**: low (message-text divergence in a diagnostic; no state, control-flow or
  rendering difference)
- **kind**: log-text divergence / cross-cutting
- **root cause (widened 2026-09-10)**: this is NOT only about error values. The general
  shape is **Rust's derived `Debug` standing in for Zig's builtin aggregate formatter**,
  and it bites plain values as much as errors. Three non-error instances, all verified
  against captured records, not inferred:
  - `image.zig:122` and `:132` — the placement id renders as `Kitty(7)` where Zig's `{}`
    on the tagged union prints `renderer.image.Id{ .kitty = 7 }`. Present on **both**
    platform twins (`image/macos_impl.rs`, `image/linux_impl.rs`).
  - `generic.zig:2137` — the screen size renders as a full Rust struct dump where Zig
    prints `renderer.Size{ .screen = … }`. Ported at
    `crates/rustty-renderer/src/generic/linux_impl.rs:1470` and its macOS counterpart.

  **Mirroring the existing behaviour was the right call and should stay that way until the
  sweep.** `Id` has no Zig `format` method to port, so hand-writing a `Display` impl would
  be *inventing* fidelity rather than restoring it; and the two platform twins must keep
  the same public surface, so a divergence introduced on one arm alone would be a worse
  defect than the text gap. These are recorded here rather than filed separately because
  the root cause is identical and splitting them would fragment the eventual sweep.
- **oracle**: every `log.*("… err={}", .{err})` in `src/renderer` and `pkg/opengl` — e.g.
  `generic.zig:852, 1436, 1558, 2627, 3461`, `renderer/Metal.zig`, `renderer/image.zig`,
  `renderer/opengl/Frame.zig:65`, `renderer/OpenGL.zig:242, 308`.
- **rustty**: `crates/rustty-renderer/src/**` — 31 sites in `generic.rs`, plus `metal/*`,
  `image/macos_impl.rs`, `overlay.rs`, `opengl/opengl_impl.rs`, `generic/linux_impl.rs`,
  `opengl/shaders.rs`. ~35 in total at the time of writing.
- **description**: Zig's `{}` applied to an error VALUE prints `error.` **plus** the name
  (`std.fmt`'s error-set arm), so the oracle emits e.g. `… err=error.OpenGLOutdated`. The
  port's uniform convention is `err={err:?}`, i.e. the Rust error's `Debug`, which prints
  `OpenGLOutdated` — or, for a tuple variant, `EglUnavailable("…")`. The prefix, and
  sometimes the shape, differ. Task-14c-1 already hit and fixed this for ONE site where
  the rendering was knowable and hard-codeable (`error.StreamTooLong`, its fix round 1);
  every other site formats an arbitrary error value and was left as `{err:?}`.
- **the pattern already exists in this tree**: `crates/rustty-font-shaper/src/shaper/
  web_canvas.rs:455-466` gives its error type a small `Display` impl that writes
  `error.{name}` with the comment "Zig renders a caught error as `error.Name` under
  `{}`". `rustty-termio/src/command.rs:180` and `:1511` and
  `rustty-apprt/src/gtk/cgroup.rs:51` do the same. Applying that shape to the renderer's
  error types and switching the sites from `{err:?}` to `{err}` is the fix.
- **how found**: task-14c-3, restoring `OpenGL.zig:242`/`:308` and noticing its own two
  new sites inherit the divergence. Raised in that task's report; recorded here in its
  fix round 1 at the coordinator's instruction, because it is crate-wide and does not
  belong inside any one closed row.
- **why this is recorded separately**: it is not a defect any single row introduced or can
  close. No row's `CLOSED`/`WAIVED` state rests on it, and churning ~35 sites inside a fix
  round is exactly the kind of unscoped change this plan avoids.
- **state**: **OPEN — scope recorded, stopped under the 2-hour rule** (G-1, 2026-09-28). Done toward it: the shared mechanism now exists, `rustty_zigstd::error_name` (`ErrorName` = `@errorName`, `ZigError` renders `error.<Name>`, wrappers name the error they wrap because a Zig union is flat), with impls for the font-shaper collection/face/discovery/grid errors (N121), and the one Metal site whose error set was total (`shaders.zig:151`, N8) prints it. Left, measured on this tree (`grep` of `{err:?}`/`err={:?}`/`{e:?}` outside comments and test helpers): 65 arguments in `crates/rustty-renderer/src` still render Debug: macOS-compiled `generic.rs` 22 (display link x3: `metal::cvdisplaylink` errors; background image open/read x2: `io::Error`, which needs the `std.Io` call's errno table like `rustty_config::zig_error`; row/preedit/underline/overline/glyph/strikethrough/cursor x2/preedit glyph: `shared_grid::RenderError` → `codepoint_resolver::Error` → `rustty_font::sprite::face::RenderGlyphError` → `rustty_z2d` errors, none of which implement `ErrorName` yet; OSC8/regex links and search highlights: `rustty-vt`/oniguruma errors; overlay surface/images; present; custom shaders), `image/macos_impl.rs` 7 (GPU upload, vertex buffer, kitty placement x3, virtual placement, image prep; plus the non-error `image::Id` aggregate at `image.zig:122`/`:132`), `overlay.rs` 3 (z2d), `metal/cvdisplaylink.rs` 1, `metal/metal_impl.rs` 2; Linux-only twins `generic/linux_impl.rs` 19 (plus `renderer::Size` at `generic.zig:2137`), `image/linux_impl.rs` 7, `opengl/opengl_impl.rs` 3 and `opengl/shaders.rs` 1, whose two tests (`display_realized_warns_and_continues_when_context_prep_fails`, `present_logs_when_the_srgb_re_enable_fails`) pin the Debug form. Why stopped: each site needs its error type traced to the reference call's error set (about 15 error types across rustty-renderer, rustty-font, rustty-z2d, rustty-vt, rustty-oniguruma, the xev/CoreVideo seams and `io::Error`), and the twins must change together, which needs a Linux (`rvm`) build and test run for the half that does not compile on macOS. Estimate: 4-6 h including the Linux lane. Was: **OPEN** (triage T-1, 2026-09-28). Unchanged: 59 `err={err:?}` / `err={:?}` sites remain in `crates/rustty-renderer/src`, and no renderer error type has the `error.<Name>` `Display` (the pattern exists in `rustty-config::zig_error`, `rustty-cli`, `web_canvas.rs`). Left: the scope written below. Effort M, no long run; observable only in diagnostic text (R1), no state or rendering difference. Was: **OPEN**, scheduled as its own task. Scope: give the renderer's error types
  (`opengl::opengl_impl::Error`, `opengl::gl::errors::Error`, `metal::*`, the shaper and
  collection errors that reach a log site) the `error.{name}` `Display` impl, switch the
  sites to `{err}`, **and decide the non-error aggregates named under root cause above**
  (`image::Id`, `renderer::Size`) in the same pass, on both twins together, and update the handful of tests that assert on a rendered payload —
  `opengl_impl::tests::display_realized_warns_and_continues_when_context_prep_fails` and
  `present_logs_when_the_srgb_re_enable_fails` both pin the current `Debug` form and will
  need their expected strings changed with it.
## N12 — the audit's line anchors leave whole reference-file HEADS unaudited, and no row will ever claim them (new gate G-LOGFILE)

- **id**: N12 (next free id checked 2026-09-10 against `NEW-FINDINGS.md`, `ADJUDICATION.md`
  and the plan's `progress.md`: N1-N11 taken, N12/N13 pre-allocated in `progress.md` for
  exactly these two entries and used here as allocated.)
- **severity**: medium (a systematic audit-coverage hole, not one defect; it has already
  produced real gaps — N4, N6, N7 and N9 are all instances)
- **kind**: audit-coverage / gate blind spot
- **oracle**: every reference file cited by a `LOG`-class ledger row, at pin `20abdb50a`.
- **rustty**: n/a — this is a finding about the AUDIT and the GATE, not about a code site.
- **description**: `G-PARITY` counts ROWS. Every audit row cites a RANGE of reference line
  numbers, so anything OUTSIDE every row's range is unaudited and **no row will ever claim
  it**. Several large reference files have their lowest cited anchor partway down the file,
  leaving an entirely unclaimed head. Measured by sweeping every `LOG` row, taking each
  row's minimum anchor, and counting the reference's own live `log.*` calls above it:

  | unclaimed head sites | oracle file | lowest row anchor | rows |
  |---|---|---|---|
  | 38 | `src/Surface.zig` | 2193 | S41-F1 (WAIVED), S42-F2 (CLOSED) |
  | 20 | `src/apprt/gtk/class/application.zig` | 812 | S49-F5, S50-F4 (both OPEN) |
  | 14 | `src/config/Config.zig` | 4680 | S74-F5, S75-F1, S75-F2 (all OPEN) |
  | 8 | `src/apprt/gtk/winproto/x11.zig` | 225 | S60-F6 (OPEN) |
  | 7 | `src/renderer/OpenGL.zig` | 237 | S113-F1, S113-F4 (both CLOSED) |
  | 5 | `src/main_ghostty.zig` | 129 | S44-F4 (WAIVED) |
  | 2 | `src/terminal/stream.zig` | 1352 | S163-F2, S164-F1 (both CLOSED) |
  | 2 | `src/apprt/gtk/class/surface.zig` | 1045 | S54-F4, S54-F6 (both OPEN) |
  | 1 each | `terminal/page.zig`, `terminal/Terminal.zig`, `renderer/Thread.zig`, `font/CodepointResolver.zig`, `apprt/embedded.zig`, `font/glyf_rasterize_png_test.zig` | | |

  **101 unclaimed head sites over 14 files.** This table was independently reproduced from
  the ledger and the pin by task 14gate and matches the original review count exactly.

  **A head being unclaimed does NOT mean it is unported.** Three of the six largest are
  already restored and a gate that flagged them would be ignored within a day:
  `Config.zig`'s head is live (`crates/rustty-config/src/config_struct.rs:5472, 5949, 5984,
  6151` — through that file's `log_info`/`log_warn` fn facade, not `log::`);
  `OpenGL.zig`'s head ports to `crates/rustty-renderer/src/opengl/gl/sys.rs:400, 409,
  578-582` — a DIFFERENT Rust file than the name suggests, and through a `macro_rules!`
  facade; `Surface.zig`'s head was restored by task 14d-1.

- **the fix is the gate, not the rows**: `.port/tools/gates/check_log_reconcile.py`
  (`G-LOGFILE`) reconciles per ORACLE FILE, comparing the reference's live `log.*` call
  sites against live port emissions in the Rust file(s) declared to port it
  (`.port/parity-closure/LOG-PORTMAP.tsv`). Because it counts the WHOLE file it does not
  care where a row's anchors fall, so an unaudited head cannot hide in it. It is HARD in
  `ci.sh` from day one.

- **what the gate measured, and this is the tracked debt**: 63 reference files, 894 live
  reference log sites, 753 live port emissions on the thinnest platform arm —
  **a deficit of 183 sites over 33 files**, frozen as the portmap's allowances. Full table:
  `python3 .port/tools/gates/check_log_reconcile.py --report`. The ones that are NOT
  explained by an already-OPEN row:

  | deficit | oracle file | rows | note |
  |---|---|---|---|
  | 7 | `src/terminal/stream_terminal.zig` | S166-F3 — **CLOSED** | the clipboard reply/grant family |
  | 3 | `src/terminal/Terminal.zig` | S141-F4, S142-F1, S143-F2 — **all CLOSED** | `cursor copy failed entering alt screen`, the grapheme-attach debug |
  | 2 | `src/font/face/coretext.zig` | S90-F2 — **CLOSED** | |
  | 2 | `src/font/face/freetype.zig` | S91-F8, S91-F10 — **both CLOSED** | |
  | 1 each | `PageList.zig`, `page.zig`, `kitty/graphics_storage.zig` | CLOSED | |

  A `CLOSED` row sitting above a live deficit is exactly PROCEDURE.md blind spot 2 ("the
  gate counts rows, not sites") measured for the first time. These are **not** closed by
  this entry.

- **CORRECTION, 2026-09-10 (task 14gate fix round 1).** The figures first written here were
  **232 over 34 files**, with `src/terminal/stream.zig` as the largest single instance at
  **49**, attributed to "repeated message families collapsed onto one port site". **That
  defect does not exist.** `stream.zig` reconciles: 128 reference sites against **138** port
  emissions. The 49 was a scanner defect, not a port defect.

  `check_log_reconcile.py`'s Rust stripper had no char/byte-literal branch.
  `crates/rustty-vt/src/stream.rs:3428` is `b'"' => {` — the DEC intermediate. That quote
  opened a phantom string, everything to the next `"` was blanked as string body, and the
  desync ran to end of file: ~1200 lines of real code erased before the scan, 59 live
  emissions invisible, all of them at line 3446 or later. The Zig side always handled char
  literals correctly (`zig_strip`); only the Rust side forgot, and Rust is where it matters,
  because that stripper is whole-file while a Zig desync cannot escape its line.

  Two things worked and one did not.
  - **The ratchet worked.** With the scanner fixed, the gate went RED by itself:
    `src/terminal/stream.zig: allowance=49, measured deficit=0`. The stale-allowance arm is
    what surfaced the phantom, exactly as designed.
  - **The synthetic self-test did not, and could not.** All 16 arms stayed green through
    the whole thing, because a scanner desync depends on real file content. The gate now
    carries a **live-tree arm** that checks the scanner against a naive grep over every
    mapped port file and fails if any raw `log::` occurrence is dropped without a comment
    marker to account for it. Reverting the fix makes that arm fail, first violation at
    `stream.rs:3446`.
  - **Blast radius, verified independently by running every mapped port file through both
    the old and the fixed stripper**: 33 of 68 files' stripped TEXT differs (any file with a
    char literal), but only `stream.rs` differs in emission COUNT — 79 -> 138. The other 32
    files' counts and allowances were and are correct. The bug was latent and general
    though: any port file that later gains an odd number of quote-bearing char literals
    would silently zero everything below it, and it fails FALSE-GREEN.

- **three port-side facts a naive scanner gets wrong**, all found while building the gate
  and all of which cause a FALSE report if missed:
  1. **There are four emission shapes, not one.** `log::warn!(target: SCOPE, …)` is only
     the commonest. `crates/rustty-renderer/src/opengl/gl.rs` defines `macro_rules!
     log_warn!` and its leaf modules emit through that; `crates/rustty-config/src/
     config_struct.rs` and much of `rustty-os` use a `fn log_warn(msg)` facade;
     `crates/rustty-vt-c/src/sys.rs::log_fn` ends in an embedder-installed **C callback
     pointer** and contains no Rust write primitive at all. A `log::`-only scan reads
     `rustty-vt-c` as emitting nothing when it in fact reconciles exactly (2/2, 5/5, 1/1,
     1/1) and reads `gtk/class/application.rs` as **zero** when it emits **96**.
  2. **`eprintln!` is a real emission in the GTK tree** — `rustty-apprt`'s GTK files
     reproduce Zig's default `logFn` OUTPUT inline, prefix and all
     (`eprintln!("warn(gtk): …")`). The prefix is the signal: a bare `eprintln!` is
     ordinary program output (help-text failures, test-harness skip lines) and counting it
     would manufacture emissions the reference never asked for.
  3. **A `log_*` helper with an empty body is a DROPPED diagnostic, not a facade.**
     `rustty-os/src/locale.rs` and `cf_release_thread.rs` carry 16 such no-op stubs between
     them, each holding the reference call in a comment and emitting nothing. The gate
     counts their call sites as zero and lists them under its own `NOTE:` line.

- **correction to the brief this task was given**: it stated that
  `crates/rustty-apprt/src/gtk/class/application.rs` has "**zero** facade emissions". It
  has **96**, through shape (2) above; the file is 15 short of the reference's 111, not
  111 short. The brief's three named false positives are all genuinely false positives at
  the HEAD level, but two of them (`Surface.zig` −3, `Config.zig` −8) do carry a whole-file
  deficit elsewhere in the file, so "the head is restored" and "the file reconciles" are
  different claims and this finding keeps them separate.

- **how found**: a review swept every LOG row's minimum anchor and counted above it; task
  14gate reproduced the sweep, then built the gate and measured the whole-file picture.
- **A FINER INSTANCE OF THE SAME CLASS, found 2026-09-10 by task-14d-2 — this entry
  describes unaudited HEADS only, and the hole is bigger than that.** The rows citing a
  file also leave unaudited GAPS *between* their cited ranges, which no minimum-anchor
  sweep can see. `Config.zig` is the worked example: its rows cite `:4331-4336`,
  `:4413-4448`, `:4671-4730`, `:4844-4849`, `:6437-6481`, `:7355-7397`, `:8199-8253`,
  `:8719-8724`, `:9002-9309` and `:10093-10790`, and four live `log.*` sites fall in the
  gaps between them — `:4563`, `:5396`, `:5400` (unrestorable, N16) and `:8887`/`:8893`
  (restorable, restored by task-14d-2 under their own heading). All five are BELOW the
  file's lowest anchor, so the head sweep counted none of them. Only the whole-file
  reconciliation `G-LOGFILE` performs catches this variant, which is a second argument for
  the gate being per-file rather than per-row. Any future sweep of this class must count
  set-difference against the union of a file's ranges, not against their minimum.
- **state**: **CLOSED** (triage T-1, 2026-09-28): the debt is worked down. The S-1 log-gate work (tail-s1, tail-s1b `00e68940`, S-1c `f5ee0e13`, S-1d `ca4b8495`; merged `588f98fc`) took G-LOGFILE's allowances from 131 units over 50 files to 3 over 2, and the per-file deficits named here (`embedded.zig`, `application.zig`, `locale.zig`, `cf_release_thread.zig`) are all 0 (`check_log_reconcile.py --report`, 2026-09-28). The 3 remaining units are N314's test-only / never-compiled sites. Was: **OPEN.** The gate and the corrected baseline are landed; the 183-site debt is
  not. Next step is a per-file adjudication pass in deficit order — `embedded.zig` (54, all
  54 reference calls survive only as R1 comments) first, then `application.zig` (15),
  `locale.zig` (14) and `cf_release_thread.zig` (9) — tightening the portmap allowance in
  the same commit as each restoration (the gate is a ratchet and goes RED on slack).

## N13 — two `self_.?` elisions turn a Zig safe-build panic into silent UB on an ABI-nullable pointer

- **id**: N13 (allocated with N12 above; ids checked 2026-09-10)
- **severity**: medium (undefined behaviour on a null pointer where the reference has a
  defined, loud failure; unreachable in practice today, but only by construction)
- **kind**: safety-check elision / uncited site
- **oracle**: `src/os/cf_release_thread.zig` `wakeupCallback` and `src/renderer/Thread.zig`
  `wakeupCallback`, both `const t = self_.?;`.
- **rustty**: `crates/rustty-os/src/cf_release_thread.rs:269` and
  `crates/rustty-core/src/renderer_seam.rs:1679` — both replace the unwrap with the comment
  `// const t = self_.?;  (non-null by construction)` and then use the raw `self_` pointer.
- **description**: the reference's `.?` on an optional pointer is a **checked** unwrap: in
  a safe build a null userdata panics with `attempt to use null value`, loudly and at the
  call site. The port elides the check entirely and dereferences `self_` unconditionally,
  so the same input is undefined behaviour instead.

  The "non-null by construction" justification is weaker than it reads, because the
  pointer is **nullable by ABI on this very path**:
  `crates/rustty-xev/src/common.rs:23-29`'s `userdata_value` returns
  `core::ptr::null_mut()` whenever the userdata type is `()`, faithfully reproducing the
  reference's `Userdata == void` special case. Any watcher registered with unit userdata
  therefore hands these callbacks a null `self_`. Today no caller does; nothing in the type
  system or in a debug assertion says so, and nothing fails loudly if that changes.
- **why it is here and not in a row**: no ledger row cites either line. It is **not a LOG
  defect** — no diagnostic was dropped at either site (both keep their `log.err` arms; the
  Thread one emits, the cf_release_thread one goes through a no-op stub tracked under N12's
  point 3). It surfaced only because both files were being read for the logging sweep.
- **the cheap fix**: `assert!(!self_.is_null(), "self_.? — the reference unwraps a checked
  optional here")` at both sites, or a `debug_assert!` plus a PORT NOTE if the release-mode
  branch is unacceptable. Either restores a defined failure and documents the invariant the
  comment currently only asserts in prose.
- **how found**: task 14gate, while enumerating `rustty-os` and `rustty-core` emission
  sites for the G-LOGFILE portmap.
- **state**: **CLOSED** (G-1, 2026-09-28). Every `self_.?` the two files elided is now a checked `assert!(!self_.is_null(), "attempt to use null value")` (R7, the invariant check ported checked; Zig's safe-build message): `cf_release_thread.rs` `wakeupCallback` and `stopCallback` (`cf_release_thread.zig:173`, `:190`), and `renderer_seam.rs` `wakeupCallback`, `drawNowCallback` and `stopCallback` (`Thread.zig:496`, `:542`, `:729`; the two besides the entry's pair have the same shape). Tests (`#[should_panic(expected = "attempt to use null value")]`, each a null userdata with `Ok(())`): `cf_release_thread::tests::{wakeup,stop}_callback_null_userdata_panics_at_the_unwrap`, `renderer_seam::thread_diagnostics_tests::{wakeup,draw_now,stop}_callback_null_userdata_panics_at_the_unwrap`. Negative controls (one at a time): with the cf `wakeupCallback` assert removed the test aborts the binary (SIGABRT) on Rust's debug null-dereference check at `cf_release_thread.rs:265`, inside `drainMailbox`, i.e. a null dereference further in (UB in release); same for the core `stopCallback` (`renderer_seam.rs:2139`). Was: **OPEN** (triage T-1, 2026-09-28). Unchanged: `crates/rustty-os/src/cf_release_thread.rs:305` and `crates/rustty-core/src/renderer_seam.rs:1690` still elide `self_.?` with a "non-null by construction" comment (the render callback's null arm was fixed separately and is tested, `renderer_seam.rs:2961`). Left: an R7 checked assertion at both wakeup callbacks plus a null-userdata test each. Effort S, no long run; internal soundness only (unreachable today, no observable parity effect). Was: **OPEN.** Not fixed here — this task is a gate, and adding assertions to two
  callbacks is a code change outside its scope.
## N14 — the GTK subtree writes 244 diagnostics straight to stderr, bypassing the log facade entirely

- **id**: N14 (next free id checked 2026-09-10: N1-N13 taken, N12/N13 by this same task.)
- **severity**: medium (a whole subtree's diagnostics cannot be silenced, levelled, scoped
  or captured; no state or rendering difference)
- **kind**: facade bypass / cross-cutting
- **oracle**: every `log.*` in `src/apprt/gtk/**` — `class/application.zig` (111 sites),
  `class/surface.zig` (63), `class/window.zig` (20), `portal/OpenURI.zig` (19),
  `winproto/x11.zig` (11) and the rest of the subtree — all of which the reference routes
  through `std.log`, i.e. through `main_ghostty.zig`'s `logFn`.
- **rustty**: `crates/rustty-apprt/src/gtk/**`. **244 raw `eprintln!` calls shaped as log
  lines** (189 `"warn(…"`, 30 `"debug(…"`, 13 `"err(…"`, 7 `"info(…"`, 5 `"error(…"`)
  across 19 files, reproducing the reference's `logFn` OUTPUT — `level(scope): message` —
  inline, character for character. **94** of them are in `class/application.rs` alone, 59
  in `class/surface.rs`, 18 in `class/window.rs`, 13 in `portal/open_uri.rs`, 12 in
  `class/global_shortcuts.rs`, 10 in `class/split_tree.rs`, 9 in `post_fork.rs`.

  **The bypass is NOT confined to the GTK tree, and an earlier revision of this entry said
  it was.** It claimed one further site outside that tree
  (`crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs:418`) and that "it is
  the only one". **False.** That sweep only searched `crates/rustty-apprt/src/gtk/**` plus
  whatever the portmap already declared, and the portmap at the time covered 63 of the
  reference's 172 log-bearing files — so the claim was scoped to what had been looked at and
  written as if it were scoped to the tree. 31 further sites exist across five files, all
  verified by hand:

  | sites | port file | reference |
  |---|---|---|
  | 16 | `crates/rustty-vt/src/search/thread.rs` | `src/terminal/search/Thread.zig` |
  | 5 | `crates/rustty-vt/src/search/terminal.rs` | `src/terminal/search/terminal.zig` |
  | 4 | `crates/rustty-vt/src/search/screen.rs` | `src/terminal/search/screen.zig` |
  | 5 | `crates/rustty-font/src/sprite/face.rs` | `src/font/sprite/Face.zig` |
  | 1 | `crates/rustty-font/src/atlas.rs` | `src/font/Atlas.zig` |

  With the portmap completed (finding N15) the gate now measures **276 facade-bypassing
  emissions across 25 reference files' ports**, all capped. That is the honest total for the
  declared universe, and it is 39% larger than the GTK-only figure this entry first carried.

  **These figures were corrected in fix round 2 and the correction is instructive.** They
  were first filed as 199 / 157-22-9-6-5, with 98 in `application.rs`, from a line-oriented
  `grep`. The real numbers come from running **the gate's own `EPRINTLN_LOGFN` regex**,
  which matches multi-line call sites (`eprintln!(` with the format string on the next
  line) because `\s*` spans newlines. So a finding filed to document a facade bypass was
  itself counted informally rather than through the instrument built to stop informal
  counting — the same class of error as the scanner bug in fix round 1. The qualitative
  finding is unaffected and was understated: the bypass is 23% larger than reported.
- **description**: the reference's `logFn` (`src/main_ghostty.zig`) is a real funnel. Its
  port is `crates/rustty-core/src/logging.rs::log_with(level, scope, message, &Logging)`,
  and it does three things these sites do not:
  1. **it honours `Logging.stderr`** (`crates/rustty-core/src/logging.rs:88` returns early
     when it is false) — and `rustty_global::Logging`'s own doc says stderr logging is
     **disabled by default in lib mode**. So an embedder that has switched stderr logging
     off still gets all 199 of these on its stderr;
  2. **it routes to macOS unified logging** when `Logging.macos` is set
     (`logging.rs:56`) — these sites never reach it, so on macOS the GTK subtree is absent
     from `log stream` entirely;
  3. **it is the single point where level and scope filtering can ever be applied.**
     A raw `eprintln!` has no level to filter on and no scope to route by.

  The facade is not merely available, it is **already imported by the same crate**:
  `crates/rustty-apprt/src/lib.rs:189` does `use rustty_core::logging::{log_with, Level};`
  and `global.zig`'s eleven sites go through it correctly. The GTK subtree simply does not,
  and `rustty-apprt/Cargo.toml` does not list `log` as a dependency at all, so the
  `log::warn!` shape used by every other crate is not even reachable from there.
- **why this is a finding and not part of N12**: N12 is about diagnostics that are MISSING
  (`gtk/class/application.zig` is 15 sites short of the reference's 111). This is about the
  96 that are PRESENT and go to the wrong place. Restoring the missing 15 as further
  `eprintln!`s would close N12's arithmetic for that file and make this defect worse. The
  two must be fixed in one pass or the wrong one will be done first.
- **how found**: task 14gate. The gate had to learn the `eprintln!("warn(gtk): …")` shape
  in order to count these as emissions at all — a `log::`-anchored scan reads
  `class/application.rs` as emitting **zero** when it emits 96 — and recognising the shape
  is what exposed what the shape costs.
- **the gate now enforces it (fix round 2).** `LOG-PORTMAP.tsv` gained a **`bypass_cap`**
  column: how many of a file's emissions may bypass the facade, frozen at today's
  measurement, MAX over platform arms (a bypass on any arm is a bypass), with the same
  ratchet as the allowances — a cap larger than what is measured is RED, so routing sites
  through the facade must tighten it in the same commit. **276 sites are capped across 25
  files** once the portmap was completed (N15). Before that completion only 188 were, in six
  files, because the portmap covered only what the audit's `LOG` rows named.

  This exists because *"restore the missing sites"* and *"stop bypassing the facade"* pull
  in opposite directions. G-LOGFILE counts a logFn-shaped `eprintln!` as an emission, so
  before this column a stage could have closed `application.zig`'s 15-site deficit by adding
  fifteen more direct prints and the gate would have reported an improvement.
- **state**: **CLOSED** (task G-2, 2026-09-28). 43 of the 51 facade-bypassing prints now go through the `log` facade under the reference scope, the crate convention (`log::<level>!(target: LOG_SCOPE, ...)`, as `stream_terminal.rs` and `gtk/media.rs` do), message text unchanged: `rustty-vt` `search/thread.rs` 16, `search/terminal.rs` 5, `search/screen.rs` 4; `rustty-font` `sprite/face.rs` 1 (the production `invalid codepoint` site); GTK `post_fork.rs` 9, `class/imgui_widget.rs` 6 (scope `gtk_rustty_imgui_widget`, `imgui_widget.zig:18`, was `gtk`), `class/inspector_widget.rs` 1 (scope `gtk_rustty_inspector_widget`, `inspector_widget.zig:13`, was `gtk`), `cgroup.rs` 1. The `bypass_cap` of those seven files is 0, `Face.zig` 4. G-LOGFILE measures **8** bypassing prints (was 51), GREEN, self-test rc 0. 8 stay by recorded decision, each with its reason in its `LOG-PORTMAP.tsv` row: the 4 in the `#[cfg(test)]` helper `test_diff_atlas_at` of `sprite/face.rs` and the one in the test-only `glyf_rasterize_png_test.rs` (Zig's test runner writes log output to stderr; the crate's test binary has no log backend, so a facade call would print nothing when the test fails), `tripwire.rs` (test-only in both trees, `tripwire.zig:128` `enabled = builtin.is_test`; `rustty-util` is the dependency-free leaf), the `rustty` build script (runs before any backend exists; cargo shows its stderr) and xtask (host tool, no backend). Tests: `search::thread::tests::drain_mailbox_logs_through_the_log_facade` (captures `(Debug, "search_thread", "changing search needle to 'world'")`), and `post_fork::tests::single_instance_detect_is_error` / `unknown_pid_is_error` now assert the full captured record (`(Error, "gtk_post_fork", ...)`). Negative controls, one at a time, each failing for the stated reason: the needle site back to `eprintln!` fails the vt test ("no search_thread needle record captured", only the mailbox record present); the `PID of child not known!` site back to `eprintln!` fails `unknown_pid_is_error` (captured `[]`, Linux `rvm`). Not covered by a test: the imgui/inspector/cgroup sites (need a GL area, a live inspector or a failing D-Bus call) and the font site (followed by `unreachable!`); their routing is the same one-line change, compiled on Linux with the GTK features. Was: **OPEN, narrowed** (triage T-1, 2026-09-28). The GTK class files now log through the facade (14d-3 `83ecca85` and later; `class/application.rs` has no bypass cap), and G-LOGFILE measures **51** facade-bypassing prints, down from 276, all capped: `rustty-vt` search 25 (`search/thread.rs` 16, `terminal.rs` 5, `screen.rs` 4), `font/sprite/face.rs` 5, GTK 17 (`post_fork.rs` 9, `imgui_widget.rs` 6, `inspector_widget.rs` 1, `cgroup.rs` 1), `tripwire.rs` 1 (`rustty-util` has no `log`), `xtask` 1 and `build.rs` 1 (no log backend), and `glyf_rasterize_png_test.rs` 1 (test-only). Left: route the vt/font/GTK sites (47) through `log::` (all three crates already depend on `log`; `post_fork.rs` runs in the parent, so the async-signal restriction does not apply) and tighten each `bypass_cap`; decide the four structural ones (tripwire, xtask, build script, test file) by a recorded reason. Effort M, no long run (GTK parts need a Linux build on `rvm`); observable only in log routing (an embedder that turns stderr logging off, and macOS unified logging, still see or miss these lines), no state difference. Was: **OPEN.** Scope: route the GTK subtree through `rustty_core::logging::log_with`
  (or add `log` to `rustty-apprt` and use `log::warn!(target: …)` as the rest of the
  workspace does), preserving each message verbatim, in the same pass that restores the
  sites N12 counts as missing. The count-based half of the gate is neutral to the fix (both
  shapes count as emissions); the `bypass_cap` half is what will move, downward, and the
  ratchet forces it to.
## N15 — the portmap was seeded from the audit's LOG rows, so it inherited the audit's blind spots: 112 reference files and 523 diagnostics were outside the gate entirely

- **id**: N15 (next free id checked 2026-09-10 against `NEW-FINDINGS.md` headings, a
  workspace grep for `\bN[0-9]+\b`, and the plan's `progress.md`: N1-N14 taken.)
- **severity**: high (not a defect in the port, a defect in the INSTRUMENT — the gate built
  to stop the audit's blind spots was reproducing them one level up)
- **kind**: gate coverage / audit-inheritance
- **oracle**: the whole reference tree at pin `20abdb50a`.
- **rustty**: `.port/parity-closure/LOG-PORTMAP.tsv`, `.port/tools/gates/check_log_reconcile.py`.
- **description**: `G-LOGFILE` reconciles the reference files listed in its portmap, and the
  portmap was seeded from the reference files cited by `LOG`-class ledger rows. **So the
  gate could only ever see what the audit had already looked at.** N12 records the
  unaudited-HEAD problem — regions of covered files that no row's line range claims. This is
  the same failure at FILE granularity, and it is much larger.

  Measured 2026-09-10 by walking the reference tree directly:

  | | before | after |
  |---|---|---|
  | reference files with live `log.*` sites | **172** | 172 |
  | live reference sites in them | **1417** | 1417 |
  | declared in `LOG-PORTMAP.tsv` | 60 (+3 with zero sites) | **170 (+2 excluded)** |
  | **undeclared** | **112** | **0** |
  | live sites the gate could not see | **523 (37%)** | **2** |
  | site coverage | **63%** | **99%** |

  The largest single log-bearing file in the entire reference — `src/termio/Exec.zig`, 58
  live sites — was not in the portmap. Nor were `termio/stream_handler.zig` (41),
  `terminal/tmux/control.zig` (19), `terminal/kitty/graphics_image.zig` (18),
  `terminal/search/Thread.zig` (18), `apprt/gtk/class/split_tree.zig` (15),
  `apprt/gtk/class/global_shortcuts.zig` (14), `termio/Thread.zig` (14).

  **And silence was indistinguishable from coverage.** Nothing in the gate's output
  distinguished "this file reconciles" from "this file was never looked at". A green
  G-LOGFILE read as "diagnostics reconcile" when it meant "the 63% we declared reconcile".

- **why the numbers here differ slightly from the review's**: the review reported 180 files
  / 120 undeclared / 550 sites, and `Exec.zig` at 61. This entry says 172 / 112 / 523 and
  58. The gap is **calls commented out in the reference itself**, which this gate
  deliberately does not count — demanding an emission the reference does not make would be a
  false positive, and the same definition is what reproduced the review's own head table
  (38/20/14/8/7/5) and its `generic.zig` inventory (33) exactly. The conclusion is identical
  either way.

- **the fix (task 14gate fix round 3)**: three parts.
  1. **The gate now enumerates the reference tree itself** rather than trusting the portmap
     to be complete. Any reference file with live `log.*` sites that is neither declared nor
     excluded is **RED**, with the count of sites it hides. A coverage hole is now a finding,
     not a silence.
  2. **The portmap declares all 172.** 170 with their port arms — resolved from PORT_PLAN's
     `crate_map`, then corroborated file by file (91 of 93 auto-resolved candidates confirmed
     by the port file naming the reference file or reproducing its message text; the
     remainder resolved by hand against the reference's own log strings, which is how
     `pkg/opengl/Program.zig` -> `opengl/gl/program.rs`, `src/main_c.zig` ->
     `rustty-apprt-c/src/lib.rs` and `src/build/*` -> `xtask/src/*` were pinned).
  3. **Two are declared EXCLUDED**, and an exclusion must name a category from a closed set
     (`no-port`, `build-tooling`, `vendored`, `unresolved`) or the gate is RED — a free-text
     excuse is how a coverage hole becomes permanent, whereas a category can be counted and
     argued. They are `pkg/sentry/build.zig` (`build-tooling`) and
     `pkg/macos/text/font_collection.zig` (`unresolved` — one debug dump in a vendored
     CoreText binding whose port target is not established; declared so the gap is counted
     rather than invisible).

- **the baseline grew, and that is the gate becoming truthful**: deficit **183 over 33 files
  -> 535 over 113 files**; facade bypass (N14) **188 over 6 files -> 276 over 25 files**;
  port emissions seen **753 -> 924**. **No diagnostic was lost and nothing regressed.** The
  ~350 additional deficit sites were always missing; the gate simply could not see the files
  they were missing from. Anyone comparing the two baselines must compare coverage first.
- **NOT fixed here, deliberately**: none of the 535 sites were restored. That is stage work
  and far larger than a fix round. The gate measures it, the portmap declares it, and the
  ratchet forces each restoration to tighten its row.
- **how found**: the fix-round-2 review, chasing N14's false completeness claim, asked why
  five obviously log-bearing files were not capped — and the answer was that they were not in
  the portmap at all, because no `LOG` row names them.
- **the exclusion list is now budgeted too (fix round 4).** Completing the portmap created a
  new lever and left it unratcheted. An exclusion RETIRES a reference file instead of
  reconciling it, so it erases that file's entire deficit: the review demonstrated that
  reclassifying `src/termio/Exec.zig` as `no-port: nothing implements this` made **58 sites
  of deficit vanish and the gate stayed GREEN** (coverage 99% -> 95%, exclusions 2 -> 3, and
  nothing counted either). Allowances ratchet and bypass caps ratchet; this did not, and it
  is the widest lever in the gate. The portmap now carries a required
  `#!exclusion-cap files=2 sites=2` directive, frozen at today's two files and two sites, on
  the same two-way ratchet: over budget is RED, UNDER budget is RED (tighten it in the commit
  that brings a file back into scope), and a missing directive is BLOCKED — a cap that can be
  disabled by deleting a line is not a cap. Re-running the review's probe now yields
  `RED: 3 excluded file(s), budget 2` and `RED: 60 excluded site(s), budget 2`.
- **two of the 110 new mappings are NOT corroborated**, and their portmap rows now say so at
  the row rather than only in a report: `src/unicode/props_uucode.zig` and
  `src/unicode/symbols_uucode.zig` were resolved by an exact filename-stem match alone — the
  Rust file neither names the reference file nor reproduces its one log message, so both
  independent checks that confirmed the other 107 failed there. One site each; accepted
  because the stem match is unambiguous, flagged because it is the weakest evidence in the
  file.
- **state**: **CLOSED** (triage T-1, 2026-09-28): the declared debt is worked down. Same evidence as N12: after the S-1 merge `588f98fc` the gate reports 161 of 164 log-bearing reference files reconciled (+3 excluded, within the budget), 99% site coverage, and allowances 3 over 2 files (N314); `termio/Exec.zig`, `apprt/embedded.zig` and `termio/stream_handler.zig` are at deficit 0 (`check_log_reconcile.py --report`, 2026-09-28). Was: **OPEN.** The instrument is fixed and the debt is declared. Working it down is a
  per-file adjudication pass in deficit order, starting with `termio/Exec.zig` (58),
  `apprt/embedded.zig` (54) and `termio/stream_handler.zig` (41).

## N16 — three `Config.zig` `log.warn` sites are UNRESTORABLE, not dropped: their Rust counterparts are infallible

- **id**: N16 (next free id checked 2026-09-10 against `NEW-FINDINGS.md`, `ADJUDICATION.md`
  and the plan's `progress.md`: N1-N15 taken, N15 by task-14gate round 3 the same day.)
- **severity**: low (three diagnostics the port cannot emit; no state, control-flow or
  rendering difference on either side)
- **kind**: unrestorable-diagnostic / unclaimed
- **oracle**: `src/config/Config.zig:4563` (`expandHome`, `global.environMap()` catch),
  `:5396` (`Replay.next`, `diag.clone` catch), `:5400` (`Replay.next`,
  `_diagnostics.append` catch) — pin `20abdb50a`.
- **rustty**: `crates/rustty-config/src/config_struct.rs` — the `expandHome` non-wasm arm
  and the `ReplayStep::Diagnostic` arm of `Replay::next`.
- **description**: each of the three reference sites is the body of a `catch |err|` arm.
  The port's counterpart for each is INFALLIBLE, so there is no failure arm to log from:
  - `global.environMap()` -> `rustty_global::environ_map()`, which returns a snapshot of
    the process-global captured environ and has no error path at all;
  - `diag.clone(arena_alloc)` -> `Diagnostic::clone_in(&self) -> Self`, which takes no
    allocator and returns no `Result`;
  - `self.config._diagnostics.append(arena_alloc, cloned)` ->
    `DiagnosticList::append(cloned)`, which returns `()`.

  Restoring any of the three would put a diagnostic on a SUCCESS path — the bug this plan
  has already shipped once (a scroll warning outside its `catch` block firing on every
  successful scroll). They are therefore recorded, not deferred.
- **how found**: task-14d-2, reconciling `src/config/Config.zig` site by site through
  `check_log_reconcile.py`. The file measures 29 reference sites against 26 live
  emissions after that task; these three ARE that residual 3, and the portmap's allowance
  and `reason` name them.
- **why this is recorded separately**: **no LEDGER row cites any of the three.** The rows
  that name `Config.zig` cite `:4331-4336`, `:4413-4448`, `:4671-4730`, `:4844-4849`,
  `:6437-6481`, `:7355-7397`, `:8199-8253`, `:8719-8724`, `:9002-9309`, `:10093-10790` —
  `:4563`, `:5396` and `:5400` fall in none of them. This is exactly the PROCEDURE.md
  blind-spot-4 / N12 class, one level finer: not an unaudited file HEAD, but unaudited
  GAPS between a file's cited ranges.
- **what would change this**: if `rustty-config` ever gives any of the three operations a
  real error path — most plausibly `environ_map()`, were the environ capture made lazy and
  fallible — the corresponding `log.warn` becomes restorable and this entry closes with
  it. A PORT NOTE at each of the three sites says so.
- **state**: **STANDING (not a defect)** (triage T-1, 2026-09-28): the three sites are declared `infallible-call` rows in `LOG-ABSENT-SITES.tsv` (`Config.zig` 4563, 5396, 5400; task S-1c `f5ee0e13`), the gate's closed-set kind for a reference arm whose port call cannot fail, and `Config.zig`'s allowance is now 0 (26 of 29, the 3 subtracted). The entry closes by itself only if one of the three operations becomes fallible. Was: **OPEN** as a tracked non-defect. `src/config/Config.zig`'s G-LOGFILE
  allowance of 3 is these three sites and nothing else; if that number ever exceeds 3,
  something else has been dropped.

---

## N17 — the `log` facade's `.default` scope (`target: ""`) rendered `warning(): msg`, not `warning: msg`

- **id**: N17 (next free id checked 2026-09-10 against `NEW-FINDINGS.md`, `ADJUDICATION.md`
  and the plan's `progress.md`: N1-N16 taken, N16 by task-14d-2 the same day.)
- **severity**: low (a two-character difference in every `.default`-scope diagnostic's
  prefix; no state, control-flow or rendering difference)
- **kind**: emission-fidelity / cross-seam spelling mismatch
- **state**: **FIXED by task-14d-3** in the same commit that found it. Recorded because the
  CLASS is worth knowing, not because anything is left open.
- **where**: `crates/rustty-core/src/logging.rs` — `AppLogger::log` (the facade -> sink
  adapter). Live victims at `crates/rustty-renderer/src/overlay.rs:349` and
  `crates/rustty-renderer/src/shadertoy.rs:833`, the workspace's only two
  `log::warn!(target: "")` sites before this stage.
- **description**: two documents describe the same thing with two different spellings, and
  nothing bridged them.
  - `DECISION-logging.md` §6 fixes the facade convention: pass `target: ""` for the
    reference's `.default` scope.
  - `logging::log_with` is the port of `main_ghostty.zig`'s `logFn`, whose `scope` is a Zig
    ENUM and whose default arm is `if (scope == .default)`. The port spells that arm
    `if scope == "default"`.

  So a `log::warn!(target: "", ...)` fell through to the SCOPED branch and rendered
  `warning(): Error drawing hyperlink border: ...` where the reference prints
  `warning: Error drawing hyperlink border: ...`. On macOS it additionally created an
  empty unified-logging category.
- **how found**: task-14d-3, converting `crates/rustty-apprt-c/src/lib.rs`'s three
  `src/main_c.zig` sites (which are `.default`-scope) from the direct sink call to the
  facade. The defect was in the destination, not in the sites being moved.
- **fix**: `scope_from_target(target)` in `logging.rs` maps `""` -> `"default"` and passes
  everything else through. Normalized at the FACADE ADAPTER rather than inside `log_with`,
  so `log_with` stays a faithful port of `logFn` (whose `scope` can never be empty) and the
  Rust-only spelling mismatch is confined to the Rust-only bridge. Test
  `empty_facade_target_maps_onto_the_reference_default_scope`; negative control run.
- **the general lesson**: when a convention is fixed in a DECISION doc and the code that
  consumes it is a port of something with a different type (an enum, here), the two
  spellings meet at exactly one function — and that function is where the port has to be
  written down and tested. No count-based gate can see this: both spellings emit.

---

## N18 — `App.keymap` is EAGER in the port and LAZY upstream, which costs `embedded.zig:251`'s diagnostic

- **id**: N18
- **severity**: low (one `log.warn` the port cannot emit; the structural difference itself
  is benign on every path the port exercises)
- **kind**: upstream-drift / structural, surfaced as an unrestorable diagnostic
- **state**: **CLOSED by task 15**, with `S46-F1` — the row that tracks the structural
  difference itself. `App.keymap` is now `UnsafeCell<Option<Keymap>>`, initialized lazily in
  `keyboardLayout`, and `embedded.zig:251`'s `log.warn("error initializing keymap err={}")`
  is live at that site. `App::init` became infallible in the same change (the reference's
  `try app.init(..)` only ever carried `config.clone`'s allocator error, and rustty's
  `clone_config` is infallible), which retired `AppNewError::Keymap` in `rustty-apprt-c`.
  `S46-F5`'s waiver drops from 8 unrestorable sites to 7 accordingly.
- **where**: reference `src/apprt/embedded.zig:243-256` (`App.keyboardLayout`) and `:194-200`
  (`App.init`); port `crates/rustty-apprt/src/embedded.rs` `App::init` / `App::keyboardLayout`
  (PORT NOTE at the latter).
- **description**: at pin `20abdb50a` the reference stores `keymap: ?input.Keymap = null` in
  `App.init` and LAZILY initializes it on the first `keyboardLayout()` call:

  ```zig
  self.keymap = input.Keymap.init() catch |err| {
      log.warn("error initializing keymap err={}", .{err});
      return .unknown;
  };
  ```

  This port initializes `keymap` EAGERLY in `App::init`, where the reference's own `init`
  does not log at all — it `try`s and propagates. So there is no arm in the port that
  corresponds to `:251`, and emitting the warning from `App::init` would put it on a path
  the reference does not log from.
- **how found**: task-14d-3, enumerating all 54 of `embedded.zig`'s reference sites through
  `check_log_reconcile.py` while closing S46-F5. `:251` is ABOVE that row's lowest cited
  anchor, i.e. the PROCEDURE.md blind-spot-4 / N12 class again.
- **what would change this**: porting the lazy keymap (`keymap: Option<Keymap>` +
  initialize-on-first-`keyboardLayout`). That is a structural port, not logging work, and
  it also changes when a keymap-init failure becomes visible to an embedder — which is why
  it is filed here rather than folded into a LOG row. **Done in task 15** (see state).

---

## N19 — a whole crate's diagnostics were invisible to G-LOGFILE: undeclared port file + an emission shape that counts as neither

- **id**: N19 (next free id checked 2026-09-10 against `NEW-FINDINGS.md`, `ADJUDICATION.md`
  and the plan's `progress.md`: N1-N18 taken, N17/N18 by task-14d-3 the same day.)
- **severity**: medium (a hole in the "99% of the reference's log sites are reconciled"
  claim, in the file the plan calls its largest single gap)
- **kind**: gate-coverage / measurement
- **state**: **FIXED by task-14d-3** for the instance found. The CLASS is open — see
  "what to do about it".
- **where**: `LOG-PORTMAP.tsv`'s `src/apprt/embedded.zig` row, and
  `crates/rustty-apprt-c/src/lib.rs`.
- **description**: two independent invisibilities stacked on the same 23 diagnostics.

  1. **The port file was not declared.** `src/apprt/embedded.zig`'s `CAPI` block ports to
     `crates/rustty-apprt-c/src/lib.rs` — a DIFFERENT CRATE from the rest of the file.
     The portmap declared only `crates/rustty-apprt/src/embedded.rs`. So 23 of the file's
     54 sites were mapped to a file that could not carry them, and no amount of work on
     the declared file could ever have credited them.
  2. **The emissions there counted as NEITHER an emission NOR a bypass.** 22 of the 23
     were live, as `eprintln!("error initializing app err={err:?}")`. `EPRINTLN_LOGFN`
     only matches an `eprintln!` whose format string carries a `warn(scope):`-style
     PREFIX. A bare `eprintln!` matches nothing, so it is not counted as an emission
     (`primitive_sites`) and not counted as a bypass (`direct_print_sites`) either.

  The combination is worse than either half. `src/apprt/embedded.zig` reported
  `54 reference / 0 port` — a maximally alarming number — while 22 of the 23 "missing"
  CAPI diagnostics were in fact being printed, just unmeasurably, from a file the gate
  had never been told about.
- **how found**: task-14d-3, closing S46-F5. The row's title claims the whole file, so
  every one of the 54 had to be located; 23 of them were not in the declared port file.
- **fix applied**: the arm is now
  `crates/rustty-apprt/src/embedded.rs+crates/rustty-apprt-c/src/lib.rs` (a declared
  overlap with `src/main_c.zig`, recorded in both `reason` cells), and all 22 live sites
  are converted to `log::` facade calls, so both ratchets can see them.
- **why this is the SAME disease N15 treated, in a place the treatment did not reach**:
  N15 fixed "the gate's FILE LIST was seeded from the audit" by deriving the reference-side
  universe from the reference tree. That closes the question "which REFERENCE files exist".
  It does not touch the other side: **the PORT-side arm of every row is still a human
  guess, and nothing checks it.** A reference file mapped to the wrong port file, or to
  only half of the right ones, reads as a deficit forever and no gate arm objects.
- **what to do about it (for the gate task)**: there is no cheap machine check for "is this
  arm the right file". There IS a cheap check for the SECOND half: a bare `eprintln!`
  inside a file that any portmap arm names is, with very high probability, a dropped
  diagnostic rather than program output — it could be listed under the existing `NOTE:`
  line the way empty `log_*` stubs already are. That would not have declared the missing
  crate, but it would have made 22 invisible diagnostics visible as a question.
- **the general lesson**: **deriving a gate's universe from the reference fixes only the
  reference side of a two-sided mapping.** N15's rule ("derive the universe from the
  reference, never from the audit") is right and incomplete: the port side of the same
  mapping is still audit-derived, and it inherits exactly the blind spots N15 was written
  to remove.

## N20 — stale pre-resync Zig comments reproduce reference log calls that no longer exist at the pin (MACHINE-CHECKED since 2026-09-11)


> **MACHINE-CHECKED SINCE 2026-09-11 (task 14gate fix round 5).** `G-LOGFILE` now rides the
> message-content scan (N21) to detect this automatically: every preserved reference comment
> whose message appears nowhere in the reference at the pin — live OR commented-out — is
> reported, and must be declared as `stale-comment` in `LOG-TEXT-DIVERGENCES.tsv` or the gate
> is RED. **21 are declared**, well beyond the three this entry was opened for.
> (Corrected 2026-09-22, task R13-D: `LOG-TEXT-DIVERGENCES.tsv` now has **17** `stale-comment` rows,
> of 28 data rows in all; `check_log_reconcile.py` reports "19 stale preserved comment(s)" covered by
> them, since a row names a file and a message and can cover more than one comment. The 21 above is
> kept as written at the time.)
>
> **Measured 2026-09-11 by walking `is_stranded_comment`'s branches in the order the code
> applies them**, over the 1581 preserved reference comments in mapped port files (down from
> 1588 now that test spans are excluded here too):
>
> | # | branch | count |
> |---|---|---|
> | 1 | message could not be extracted (empty) | 69 |
> | 2 | elided (`…` / `...`) | 17 |
> | 3 | short or one-word | 9 |
> | 4 | exact match in the reference index (live **or** commented-out) | 1459 |
> | 5 | substring of a real reference message | 0 |
> | 6 | literal-prefix match before the first placeholder | 6 |
> | 7 | **STRANDED** | **21** |
> | | **sum** | **1581** |
>
> This table has now been wrong twice. The first version attributed the filters by estimate
> (elided ~10, short ~5, substring ~24, commented-index 36) and not one component survived
> measurement, though they summed correctly. The second version was measured — but against a
> DIFFERENT branch order than the code uses (it split the index into live and commented-out
> and tested them first), so four of its six numbers were still wrong and the six summed to
> 1590 against a stated 1588. **The gate was right both times; the table about the gate was
> not.** It is now derived by executing the gate's own predicate, branch by branch, and the
> column sums to the stated total.
>
> The short-message threshold was also a real FALSE NEGATIVE at 14 characters: it suppressed
> `exec.rs:4421`'s `"DATA: {}"`, a genuine stranding. It is 6 now, which adds exactly that
> one site. So "each filter removes only a legitimately non-verbatim class" is true of the
> common case and is not a guarantee.
>
> Each declaration is classified from the message alone and is NOT individually traced to the
> upstream commit that removed it — that tracing is still open work.

- **id**: N20 (next free id checked 2026-09-10 against `NEW-FINDINGS.md`, `ADJUDICATION.md`
  and the plan's `progress.md`: N1-N19 taken, N17/N18/N19 by task-14d-3 the same day.)
- **severity**: low individually, medium as a class — each one is a standing invitation to
  introduce the INVERSE DEFECT (an emission the reference does not make), and the class is
  invisible to every gate the plan has.
- **kind**: fidelity / stale-comment
- **state**: **STANDING (not a defect, machine-checked)** (triage T-1, 2026-09-28): G-LOGFILE's stale-comment arm is HARD in `ci.sh`, and every stranded comment it finds (19 today) is declared in `LOG-TEXT-DIVERGENCES.tsv` or the gate is RED. Per-row upstream-commit tracing stays undone and is not a parity gap. **One residual found by this triage, not a class item: CONFIRMED in the reference and recorded as UD351** (parity tail U-1, 2026-09-28). The port's OSC 7 `UriHostTooLong` arm is not dead: Zig 0.16.0 `std.Uri.parse` does not validate the host (only `resolveInPlace` calls `validateHostComponent`), and `std.Uri.getHost` maps `NoSpaceLeft` to `unreachable` (`std/Uri.zig:31`). A host containing a `%` that percent-decodes to more than 255 bytes reaches it from `reportPwd` (`src/termio/stream_handler.zig:1509`) with no length check before it. MEASURED end to end at the pin: a scratch copy of `20abdb50a` with a test calling `StreamHandler.reportPwd` panics (`attempt to unwrap error: NoSpaceLeft`, stack `getHost` <- `reportPwd`); ReleaseSafe panics the same (function-level probe of the handler's exact calls), ReleaseFast hangs at 100% CPU (UB). The port keeps its silent ignore, now marked `PORT NOTE: reference defect (R13)` with tests and a negative control; the stale `UriHostTooLong` comment and its `LOG-TEXT-DIVERGENCES.tsv` row are removed (divergence cap 29 -> 28). See UD351. Was: **OPEN** as a class. The three instances found are each annotated at their code
  site and none of them received a live emission.
- **how found**: task-14e-1, restoring `rustty-termio`. The task's method is "read the Zig
  comment, add the live call beneath it" — which silently manufactures a wrong emission
  whenever the comment is stale. Enumerating the ORACLE's own `log.*` calls through the
  gate's scanner (`zig_sites`) rather than reading the port's comments is what separated
  them: each of the three has a comment in the port and NO counterpart in the reference.
- **instances**:
  - `crates/rustty-termio/src/stream_handler.rs`, the OSC 7 `UriHostTooLong` arm —
    `log.warn("failed to get full hostname for OSC 7 validation: {}", .{err})`. At pin
    `20abdb50a`, `std.Uri.getHost`'s error set is `error{UriMissingHost}` only; the arm and
    its message are from a superseded revision. The ARM still exists in the port (removing
    it is a behavioural change), so the port has a code path the reference does not.
    (U-1, 2026-09-28: the comment is now the pin's text; the arm is the port's
    handling of reference defect UD351, not a stale path.)
  - `crates/rustty-termio/src/exec.rs`, `setNonblock` — the port's comment carried
    `read thread failed to get flags err={}` plus a second line,
    `this isn't a fatal error, but may cause performance issues`. At the pin the message
    renders the errno NAME (`err=E{s}`) and there is no second line at all. Corrected to
    the pin in this task.
  - `crates/rustty-termio/src/termio_impl.rs`, `processOutput`'s cursor-reset block —
    `log.warn("failed to get current time err={}", .{err})`. No such site in `Termio.zig`
    at the pin. Left as a comment (the surrounding `Instant::now()` is infallible in Rust
    regardless, so it could never have been emitted).
- **why this is recorded separately**: it is not a defect any row introduced or can close,
  and it is not confined to `rustty-termio` — the Phase 6 resync moved the pin under a
  tree whose Zig comments were transcribed against the OLD pin, so any file the resync
  touched can carry one. The three above were found in seven files; the reference has 173.
- **what to do about it**: the check is mechanical and belongs in `check_log_reconcile.py`
  as net-new coverage (R5): for every mapped port file, extract the message STRING out of
  each commented `log.*` line and assert it appears in the declared oracle file. A comment
  quoting a message the reference does not contain is either stale or invented, and both
  are worth a RED. Scoped as its own task, not folded into a restoration stage — the fix
  for a hit is a judgement call (correct the comment, or delete the arm) and should not be
  made under time pressure inside another task.
## N21 — a message that is PRESENT but WRONG reconciles perfectly: G-LOGFILE now compares text, not only counts

- **id**: N21 (next free id checked 2026-09-11 against `NEW-FINDINGS.md` headings, a
  workspace grep and `progress.md`: N1-N20 taken.)
- **severity**: medium (diagnostic-text fidelity; no state or control-flow difference, but
  it is invisible to every check this gate had until now)
- **kind**: gate coverage / message fidelity
- **oracle**: every `log.*` call in the reference at pin `20abdb50a`.
- **rustty**: `.port/tools/gates/check_log_reconcile.py`,
  `.port/parity-closure/LOG-TEXT-DIVERGENCES.tsv`.
- **description**: every check in `G-LOGFILE` up to this point COUNTS. A message that is
  emitted but wrong passes all of them. A stage found one funnel emitting only the `err=`
  half of `"error handling VT action action={} err={}"` — it emitted, so it reconciled — and
  then **reintroduced the same class four times in its own fix commit**, dropping the space
  before a placeholder so the port emits `mode:Flags { … }` where the reference emits
  `mode: <value>`. Discovering a defect class and recommitting it in the same change is the
  strongest available argument that it needs a machine.

  The check compares each live emission's format string against the reference call preserved
  as a comment above it, with **placeholder contents normalised and nothing else** — spacing
  is the payload, because `mode: {}` versus `mode:{}` is the bug. Runs of two or more spaces
  collapse to one, because an R1 comment often wraps one reference line across two comment
  lines; a one-space-to-zero change is still caught, a three-to-one change is not, and that
  is stated rather than hidden.

- **what it found across the port**: **12 divergences**, every one of them a knowing
  decision rather than a bug — 7 R2 identity renames (`ghostty` -> `rustty` / `APP_NAME`),
  2 format-shape differences with a PORT NOTE at the site, 1 absent payload (N18's
  `value={any}`), and 2 more with their reasons at the row. **The class is therefore not
  widespread — but that is only knowable because something went looking.**

  **CORRECTION (2026-09-11).** This first read *16*, and **three of them were phantoms
  produced by the checker's own pairing rule**, declared rather than diagnosed. The first
  `pair_messages` fell back to the oldest pending comment whenever an emission did not match
  one exactly, so the moment the port adds an emission the reference lacks, or restores a
  block out of order, a later emission consumes an earlier one's leftover comment and the
  check reports a divergence between two messages that were never a pair. A declared phantom
  is worse than an undetected one: it is permanent. Pairing is now BLOCK-SCOPED with a
  two-pass assignment — every exact match first, then best-similarity above a threshold, and
  **below the threshold the emission is left unchecked rather than falsely accused**. The
  three retractions are listed in `LOG-TEXT-DIVERGENCES.tsv`'s history; one of them
  (`open_uri.rs`) was a byte-exact port whose *comment* was abbreviated, and the comment was
  made verbatim instead.
- **what it CANNOT see, stated against the right denominator**: **884 of 1279 LIVE EMISSIONS
  (69%)** have a preserved reference comment to compare against. The first version of this
  line said 72%, over a denominator that counted only direct `log::`/print sites and left
  ~150 facade-routed emissions out of BOTH halves of the ratio. Three further facts print on
  every run, because the blindness is not random:
  - **3 crates have live emissions and not one checkable pair** — `rustty-config`,
    `rustty-util`, `rustty-vt-c` — so the check cannot see them at all;
  - **5 crates emit nothing**, so they are outside the ratio entirely rather than merely
    unchecked;
  - **30% of the 395 unchecked sites sit in two files** — `gtk/class/application.rs` (87)
    and `gtk/class/surface.rs` (33) — which are also the files with the largest deficits and
    the inline reproduction of reference output. **The highest-risk text in the port is the
    least checked**, and that is the argument for preserving the reference call at a site,
    not merely its message.
- **how a legitimate divergence is declared**: a row in
  `.port/parity-closure/LOG-TEXT-DIVERGENCES.tsv` — `kind`, `port_file`,
  `reference_message`, `port_message`, `reason` — with `kind` from a closed set
  (`identity-rename`, `error-name-literal`, `abbreviated-comment`, `format-shape`,
  `parameterised`, `payload-absent`, `stale-comment`) and a mandatory reason. **Undeclared
  divergence is RED**, and so is a declaration that matches nothing in the tree — a waiver
  that outlives what it excused is how an exception becomes permanent. Rows key on the
  MESSAGE PAIR, never on a line number, because line anchors in this plan have already
  drifted up to 70 lines when restoration moved code. The entry count is capped by a
  `#!divergence-cap` directive on the same two-way ratchet as everything else here.
- **proved by mutation, not by reading**: the first version of this checker (in the stage
  that found the original defect) used substring matching, scored zero mismatches, and
  **still scored zero when the bug was deliberately reintroduced**. So this one was
  mutation-tested against the live tree before it was trusted: truncating
  `"error building row y={y} err={err:?}"` to `"error building row err={err:?}"` in
  `generic/linux_impl.rs:2231` turns the gate RED naming the file, the line and both texts,
  and so does deleting the single space in `"error building rowy={y}"`. The first mutation
  I tried caught nothing — because that site has no preserved comment, which is how the 72%
  figure above came to be measured.
- **two states of "unchecked", not one (added 2026-09-11)**. An emission with NO preserved
  comment anywhere in its block is genuinely unknowable — that is the honest 31%. An emission
  that HAS candidate comments and matches none of them above the 0.55 similarity floor is a
  different and suspicious state, and the first version of this check put both in the same
  silent bucket. That is how a message truncated all the way down to its trailing clause
  (`"error building row y={} err={}"` -> `"err={err:?}"`, similarity **0.333**) moved from
  checked to unchecked with **zero signal and a GREEN gate** — the bigger the truncation, the
  quieter it got, which is exactly backwards. Lowering the floor would only bring back false
  accusations, so the second state is now counted, printed with both texts and the similarity
  score, declarable as `kind=unpairable` with a reason, and **ratcheted**: undeclared ones
  are capped at zero. One site exists today (`window.rs:1519`, similarity 0.548) and is
  declared.
- **the test-span exclusion was CLAIMED in three artifacts before it was implemented.**
  Round 6's commit message, this finding and `PROCEDURE.md` all said test code was skipped
  "for emissions, facade-bypassing prints and preserved comments alike"; the filtering
  existed only in the emission paths, and `glyf_rasterize_png_test.rs:426` is a live
  preserved comment inside a test function that the content check was still reading. It is
  implemented now, in `commented_reference_calls`, with self-test arms for both halves —
  there were none before, which is why nobody noticed.
- **state**: **STANDING (not a defect)** (triage T-1, 2026-09-28): a HARD standing check in `ci.sh` (G-LOGFILE's message-content arm). Today 13 divergences and 19 stale comments are declared (cap 29), 0 undeclared unpairable emissions; the unreachable share (976 of 1395 live emissions checkable, 69%) is a measured coverage limit printed on every run, not a defect. N22's denominator note is settled (dead sites subtracted per site, `a2061c15`). Was: **OPEN** as a standing check. All 12 present divergences are declared with
  reasons; what is outstanding is the 31% of emissions the pass cannot reach, concentrated
  as described above, plus the note under N22 that the 1415 reference-site denominator is
  itself 8 too high.

---

## N22 — G-LOGFILE counts `log.*` sites inside reference functions that the reference never calls (CLOSED 2026-09-11 by a site-level dead-site declaration)

**Found 2026-09-10 (task-14e-2), while accounting for the last 10 sites of `src/font/shaper/`.**

`check_log_reconcile.py`'s `zig_sites()` is a LINE scanner. `zig_strip` correctly refuses to
count a `log.*` call that is **commented out** — `Surface.zig` alone carries six — but nothing
in it can see that a whole **function** is dead. Two reference files have one:

| reference | function | its only call site, at pin `20abdb50a` | sites counted |
|---|---|---|---|
| `src/font/shaper/coretext.zig` | `debugPositions` (`:690`) | `:503` — `//try self.debugPositions(...)` | 4 (`:775, :793, :806, :820`) |
| `src/font/shaper/harfbuzz.zig` | `debugPositions` (`:302`) | `:236` — `//try self.debugPositions(...)` | 4 (`:379, :396, :409, :421`) |

So **8 of the tree's 1415 "live reference log sites" are diagnostics the reference itself
never emits.** They are counted as deficits against the port, and any stage that "restores"
them creates eight emissions the reference does not make — the inverse defect (N20's class,
reached by a different route).

**Not fixed here, deliberately.** Teaching the scanner about dead functions means resolving
Zig call graphs, which is a much larger scanner than this gate has, and R5 requires a gate
change to be net-new coverage with a self-test arm and an argument. The two files' portmap
`reason` cells now say precisely this instead, so the allowances are not read as "8 dropped
diagnostics".

**The generalisable point.** A line-oriented liveness test on the REFERENCE side has the same
blind spot the port-side scanner had before its char-literal fix: it is exact about the syntax
it examines and silent about the context around it. When a gate measures "the reference does
X", check whether X is *reachable*, not only whether it is *written*.

The port is consistent with this reading already: `coretext.rs` and `harfbuzz.rs` both preserve
`debugPositions` verbatim as a comment with a PORT NOTE saying it is never called upstream.
Making it live would additionally need `unicode.table.get(cp).grapheme_boundary_class`, which
`rustty-unicode` does not carry (checked: no `grapheme_boundary_class` anywhere in that crate).

**state**: **CLOSED** (triage T-1, 2026-09-28): as the heading says, `a2061c15` (14gate round 8) subtracts the eight dead `debugPositions` sites per site; they are `LOG-DEAD-SITES.tsv` `unreachable-private` rows (coretext.zig 775/793/806/820, harfbuzz.zig 379/396/409/421), and both files report deficit 0. Was: OPEN as a measurement caveat. No port change is owed.

**upstream record**: the dead helpers themselves are `UPSTREAM-DEFECTS.md` UD33 (task R13-R).

---

## N23 — `src/config/CApi.zig` was 11-short and no row claimed it; 7 restored, 4 unrestorable

**Found 2026-09-10 (task-14e-2).** The file entered the portmap with N15 and no `LOG`-class
row cites it, so nothing in the audit ever looked at it.

All eleven `catch` arms wrote a bare `eprintln!`. That is worse than it looks: `logFn`'s
stderr branch is gated on `global.logging().stderr`, which `global.zig`'s `init` sets **false
when a CLI action is detected** precisely so action output stays clean — and a bare
`eprintln!` ignores that gate entirely. So `rustty config --help`-style paths could emit
diagnostics the reference suppresses, on top of losing the `error(config): ` prefix.

Seven are restored through `config_struct::log_err`, the crate's own sink (§6r4's one
documented exception to the facade). `log_err` is new; `log_info`/`log_warn`/`log_debug`
already existed, which is itself worth noting — the crate had no `err`-level sink at all, so
every `log.err` site in `rustty-config` had nowhere to go.

The remaining four are the **N16 class — UNRESTORABLE, not dropped**:


> **CLOSED 2026-09-11 (task 14gate round 8).** The eight sites are declared in
> `.port/parity-closure/LOG-DEAD-SITES.tsv` and subtracted from the site inventory and from
> their files' reference counts, so **the denominator is 1407, not 1415**, and every coverage
> figure derived from it is corrected. NOT via the file-level `DECLARED-EXCLUDED` mechanism:
> `coretext.zig` still has four genuinely live sites at `:254, :264, :277, :291`, so retiring
> the file would have erased real debt and spent the exclusion budget on something that is
> not an exclusion. The declaration is per SITE, a bogus line is RED, the kind comes from a
> closed set, a reason is mandatory, and the population is capped at 8 on a two-way ratchet
> because an undeclared dead site is undetectable by construction.
>
> **And it is net-new coverage, not a subtraction (R5): a port emission carrying a
> declared-dead site's message is RED.** Nothing else in this gate can catch someone
> faithfully restoring a call the reference never makes — every count check would applaud it,
> and "read the preserved comment and add the call beneath it" is the standard method that
> produces exactly that. Proved on the live tree: adding
> `coretext.zig:775`'s message to `crates/rustty-font-shaper/src/shaper/coretext.rs` turns
> the gate RED naming both sites.

- `CApi.zig:17` / `:40` `error allocating config err={}` — the reference's `state.alloc.create`
  failure arms. `Box::new` aborts on OOM rather than returning an error.
- `CApi.zig:22` `error creating config err={}` — `Config::default_config()` returns a `Config`,
  not a `Result`.
- `CApi.zig:45` `error cloning config err={}` — `clone_config()` likewise.

Each has the reference's full sequence preserved as a comment plus a PORT NOTE saying
explicitly not to "restore" it, and the portmap `reason` records the same. Allowance 11 -> 4.

**state**: the four are a standing, documented allowance. Nothing is outstanding.

---

## N24 — a WAIVED row whose "the port cannot produce this failure" rests on a *tracked defect*, not on a language property

**Recorded 2026-09-10 (task-14e-2 fix round 1), confirmed independently by that stage's review.**

`S94-F4` is `WAIVED` on two `src/font/shaper/coretext.zig` sites — `endFrame`'s
`toOwnedSlice` catch (`:277`) and its separate `wakeup.notify` catch (`:291`) — because the
port has no failure arm to hang them on. That is true, and it is **not the same kind of claim**
as the other four waivers of this shape recorded in the same stage:

| waiver | why the port cannot produce the failure | can it ever change? |
|---|---|---|
| `CApi.zig:17,22,40,45` | `Box::new` aborts on OOM; `default_config()`/`clone_config()` return a `Config`, not a `Result` | **no** — a language property |
| `PageCompression.zig:385` | `to_vec()` aborts on OOM | **no** — a language property |
| **`S94-F4`'s two** | the port replaced the reference's `CFReleaseThread` with an **mpsc stand-in**: `std::mem::take` allocates nothing and returns no `Result`, and an mpsc `send` IS the wakeup, so there is no separate notify step | **yes** — and `S94-F1` (CONFIRMED, OPEN) is the row that tracks undoing it |

So one of the port's own open defects is what makes the other row "not a defect". Restore the
`CFReleaseThread` and both failure arms return with it.

**Why this is not simply reopened as two OPEN rows.** The arms are not independently fixable —
they come back with `S94-F1`'s swap or not at all — so a second OPEN row would be a second
name for one piece of work, on a board that is already RED at 327. What was actually wrong was
that `WAIVED` read as final. It now reads as **contingent**, in the three tracked places
`PROCEDURE.md`'s carve-out standard requires: `S94-F4`'s own `owner` cell, the PORT NOTEs at
both sites in `coretext.rs`, and this entry — so the finale re-audit meets it as an item
rather than as a footnote inside a waived row.

**The generalisable rule.** A waiver's justification has a *kind*, and the kind decides whether
the waiver can expire. "Rust's allocator aborts" never expires. "The port does X instead" expires
the moment someone fixes X — and if nothing records the dependency, the waiver outlives the
reason for it. **When waiving on "the port cannot produce this failure", say WHY it cannot, and
if the why is another row, name that row in the `owner` cell.** This is the near neighbour of
the defect the 14e-1 review caught (a waiver contradicted by an open `missing-error-path` row);
here the ledger does not contradict the waiver, it *dates* it.

**state**: **CLOSED** (triage T-1, 2026-09-28): discharged by task T-F in the change that closed `S94-F1` (`c539d380`), as the DISCHARGED paragraph below records; the `wakeup.notify` arm is restored and the remaining `toOwnedSlice` waiver now rests on a language property. Was: OPEN as a dependency. Discharged by whoever closes `S94-F1`, in the same change.

**DISCHARGED (task T-F, 2026-09-23, in the change that closes `S94-F1`).** The shaper now runs
the real `CFReleaseThread`. The `wakeup.notify` arm (`:291`) came back and is restored, and
`LOG-PORTMAP.tsv`'s allowance for the file is tightened 2 -> 1. The `toOwnedSlice` arm (`:277`)
did not come back: the port still moves the pool out with `std::mem::take`, which allocates
nothing and returns no `Result`, so that site's waiver now rests on a language property, like
the other four in the table. The table row above is kept as the record of what was believed.
## N25 — five `rustty-font` diagnostics live only in test code, and were counted as if they were the port

- **id**: N25 (next free id checked 2026-09-11 against `NEW-FINDINGS.md` headings, a
  workspace grep for `\bN[0-9]+\b` and `progress.md`: N1-N24 taken.)
- **severity**: low-medium (five reference diagnostics are absent from every production build;
  no state or control-flow difference, and nothing regressed — this is debt that was
  previously miscounted as done)
- **kind**: unrestored sites / measurement correction
- **oracle**: `src/font/sprite/Face.zig` (4 live sites) and
  `src/font/glyf_rasterize_png_test.zig` (4 live sites).
- **rustty**: `crates/rustty-font/src/sprite/face.rs` and
  `crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs`.
- **description**: until 2026-09-11 `G-LOGFILE` counted emissions inside `#[cfg(test)]` /
  `#[test]` spans as if they were production code. When test spans were excluded
  (task 14gate round 6), these two files' counts fell:

  | port file | counted before | production only | test-only |
  |---|---|---|---|
  | `sprite/face.rs` | 5 | **1** | 4 |
  | `glyf_rasterize/glyf_rasterize_png_test.rs` | 1 | **0** | 1 |

  Their portmap allowances rose accordingly — `sprite/Face.zig` 0 -> 3,
  `glyf_rasterize_png_test.zig` 3 -> 4. **Nothing regressed and no diagnostic was removed.**
  Five of the reference's diagnostics were never emitted by a production build of
  `rustty-font`; the gate simply could not tell, because a test that writes a logging macro
  to prove one fires looks exactly like a site that fires.
- **why it is recorded here rather than left in the allowances**: a raised allowance with a
  generic reason is debt nobody owns and nobody meets. This entry is so the next stage
  working `rustty-font` meets it as a tracked item. The two rows' `reason` cells point at the
  cause; this entry is the item.
- **what closing it looks like**: move the four `sprite/Face.zig` sites and the one
  `glyf_rasterize_png_test.zig` site out of test scaffolding and onto the production paths
  that correspond to the reference's, then tighten both portmap allowances in the same commit
  — the gate is a ratchet and will go RED on the slack if they are not.
- **how found**: task 14gate round 6, as a side effect of implementing test-span exclusion;
  surfaced as a finding in round 8 at the coordinator's instruction, because it had been left
  visible only as a raised number.
- **state**: **CLOSED** (triage T-1, 2026-09-28): task tail-s1b (`00e68940`) showed the reference's own sites are test-only (`sprite/Face.zig` :297/:382/:400 are in the private test helper `testDiffAtlas`; every `glyf_rasterize_png_test.zig` site is in a test), so there is no production diagnostic to move. `sprite/Face.zig` is 1/1 production site live and both allowances are 0. The 5 + 1 direct prints left in those test paths are N14's bypass caps, not a deficit. Was: **OPEN.**

## N26 — the `checkGhosttyHEnum` tautology is four crates wide, not one file

- **id**: N26 (next free id checked 2026-09-10 against `NEW-FINDINGS.md` headings and a
  workspace grep for `\bN[0-9]+\b`: N1-N25 taken.)
- **severity**: medium (four more C-enum-sync tests that could not fail; same class as
  `S45-F1`, which cites only `src/apprt/action.zig`)
- **kind**: weakened-assertion / verification instrument
- **oracle**: `src/renderer.zig:52`, `src/apprt/ipc.zig:28,182`, `src/terminal/osc.zig:246`
  (each a `lib.checkGhosttyHEnum(T, "GHOSTTY_…_")` call).
- **rustty**: `crates/rustty-renderer/src/lib.rs` (`health_tests`),
  `crates/rustty-apprt-api/src/ipc.rs` (`tests`),
  `crates/rustty-vt/src/osc/mod.rs` (`tests`).
- **description**: `S45-F1` records that `run_henum` in `action_tests.rs` synthesized the "C
  header" side of `check_ghostty_henum` from the very descriptor under test, so all 17
  `ghostty_h_*` tests compared the enum to itself. The audit scoped the row to
  `src/apprt/action.zig`. Enumerating the port's **five** `check_ghostty_henum` call sites
  showed four of the five had the identical defect — the three files above plus
  `action_tests.rs` — **21 tests total**. Only `crates/rustty-vt/src/mouse.rs` read the real
  header, and it was the only site with a `*_probe_can_fail` self-test.
- **how found**: task 15, by grepping every caller of `check_ghostty_henum` rather than only
  the file the row cites. This is blind spot 3 (the anchors locate a region, they do not
  enumerate the work) applied across FILES rather than within one.
- **state**: **CLOSED** in the same commit as `S45-F1`. All five callers now parse the named
  enum block out of `include/rustty-embed.h` via the new shared
  `rustty_lib::c_header` module, and each of the four fixed files carries a
  `*_probe_can_fail` self-test that perturbs the header text three ways (dropped, renamed,
  revalued enumerator) and asserts the check goes red. The real header was also perturbed
  live once (`RUSTTY_ACTION_NEW_TAB = 9`) to prove the unmodified test fails.
- **upstream record**: the reference's own one-directional check (a header enumerator with no
  Zig field is never examined) is `UPSTREAM-DEFECTS.md` UD16 (task R13-R).

## N27 — the vt enum-discriminant checker pairs header and Rust POSITIONALLY, so a header reorder is invisible

- **id**: N27
- **severity**: low-medium (a real gate, binding on values, blind to one drift class)
- **kind**: gate blind spot
- **rustty**: `.port/tools/check-vt-enum-discriminants.mjs:282-289`.
- **description**: the checker emits
  `_Static_assert((int)<header enumerator i> == <rust variant i's discriminant>)` by INDEX.
  It is genuinely independent — clang evaluates the header's own enumerators — and it does
  catch a value skew and a count skew. But because the pairing is positional and never
  compares the two NAMES, a transposition can be invisible: the assert is re-aimed at
  whichever enumerator now occupies that slot.

  **Correction (task 15 fix round 1, from the reviewer): this is confined to
  IMPLICIT-value enums.** Transposing two enumerators that carry EXPLICIT values moves the
  values with the names, so slot `i` now holds a different number and the assert fails —
  those enums are caught. It is the implicit-value case that is blind: with bare
  enumerators the value is the POSITION, so a transposition renumbers both names in place
  and every positional assert still holds. Most of the curated headers' enums are bare,
  which is why this matters, but the entry previously implied all of them were exposed.
- **why it is positional today**: the header's enumerator names and the Rust variant names
  differ in shape, so a name pairing needs the same `<prefix><UPPER_SNAKE>` derivation
  `check_ghostty_henum` uses. The mapping doc-line convention the checker already relies on
  (`C: Rustty…`) does not carry a prefix.
- **what closing it looks like**: derive the expected header enumerator name from the Rust
  variant name plus a per-enum prefix (declared beside the existing `PREFIX_OK` table), and
  FAIL when the positional partner's name does not match the derived one. Add a self-test arm
  that transposes two enumerators in a fixture header and asserts RED — without that arm the
  fix is unproven.
- **how found**: task 15, auditing every ABI gate for the `S45-F1` tautology at the
  controller's instruction. No other gate synthesised its expectations: `check_abi_schema.sh`
  validates the built dylib's emitted manifest against a hand-written JSON Schema and runs a
  negative control every invocation; `check_embed_h_abi.sh` clang-probes the real header;
  `check_rustty_h_abi.sh` diffs the product umbrella against the pinned oracle header;
  `check_rustty_vt_headers.sh` and `check-vt-struct-layouts.mjs` compile the real headers;
  `check_closed_enum_abi.sh` enforces a structural rule over Rust source and has no
  "expected" side to synthesise.
- **state**: **CLOSED** (G-1, 2026-09-28). `.port/tools/check-vt-enum-discriminants.mjs` step 4: each positional pair must NAME the same thing: the enumerator is `<prefix><SUFFIX>` with SUFFIX (underscores removed) equal to the Rust variant upper-cased (underscores removed), cut at a `_` boundary, and one `<prefix>` for the whole enum; the variants named differently are declared in `NAME_ALIASES` with reasons (7 `RusttySgrAttributeTag` colour tags, `RusttyTerminalOption::SizeCb`, and a strip rule for `RusttyKey`'s `KeyA`..`KeyZ`), stale rows FAIL. `--self-test` (run by `check_rustty_vt_headers.sh` arm (e) before the real check) builds fixture trees: matching fixture green; two implicit-value enumerators transposed: value asserts still compile (the blind spot) and the name arm is RED; a non-boundary suffix RED; a stale alias RED. Real tree: 641 discriminants and names across 70 enums OK; the gate (A5) GREEN. Negative control: `RUSTTY_KEY_BACKQUOTE`/`RUSTTY_KEY_BACKSLASH` swapped in `key/event.h`: the new checker fails naming both slots, the previous checker (HEAD) reports 641 OK. Was: **OPEN** (triage T-1, 2026-09-28). Unchanged: `.port/tools/check-vt-enum-discriminants.mjs` still pairs positionally and never compares names. Left: derive the expected header enumerator name per Rust variant (a per-enum prefix beside `PREFIX_OK`), fail on a name mismatch, and add a self-test arm that transposes two implicit-value enumerators in a fixture header. Effort S, no long run; internal gate quality only. Was: **OPEN.**

## N28 — two log-sink tests could fail SPURIOUSLY: they asserted over every OTHER test's diagnostics

- **id**: N28 (next free id checked 2026-09-10 against `NEW-FINDINGS.md` headings and a
  workspace grep for `\bN[0-9]+\b`: N1-N27 taken, N26/N27 by this same task.)
- **severity**: medium (a red CI run with no defect behind it, about one `cargo test -p
  rustty-vt-c` in three — and a flaky gate is a gate people learn to re-run)
- **kind**: verification instrument
- **rustty**: `crates/rustty-vt-c/src/render/tests.rs`
  (`render_invalid_selector_warns_through_the_scoped_log_sink`) and
  `crates/rustty-vt-c/src/lib.rs`
  (`log_root_tests::root_wrapper_invalid_selectors_warn_through_the_c_log_callback`).
- **description**: both tests install a process-global C log callback, provoke diagnostics
  with a SEL value no other test uses, and then — before filtering — looped over **every**
  entry in the shared capture buffer asserting `LogLevel::Warning`. The serialization guard
  they take (`sys::tests::reset_global()`) serializes tests that MUTATE the sys global; it
  does not, and cannot, stop a concurrently running test from EMITTING a diagnostic while
  the callback is installed. Any `debug`-level line from anywhere in the binary therefore
  failed them: `left: Warning, right: Debug`.

  Both tests' own comments already say the unique SEL exists "so the capture can be
  filtered" — the blanket loop contradicted the design directly above it.
- **fix**: filter FIRST, then assert level/scope/order/text over the entries the test itself
  caused. No coverage is lost: every diagnostic each test provokes is still checked on all
  four axes. What was removed is an assertion about other tests' output, which neither test
  controls or has any business making.
- **how found**: task 15's three-consecutive-runs repetition pass. One run in three was red;
  10 runs at the task's base commit (`7419838b`) were green, so the new test added in the
  same task perturbed the scheduling enough to expose it — it did not cause it. 12/12 green
  after the fix.
- **the general point, and it is the mirror image of this plan's running theme**: the plan
  has been finding instruments that could not FAIL. This is an instrument that could fail
  for a reason unrelated to its subject, which corrodes trust in exactly the same way —
  a test that is red for a reason nobody can act on trains people to ignore red. **An
  assertion over shared process-global state must be scoped to the state the test itself
  produced.**
- **fix round 1 (task 15)**: a THIRD instance was found at
  `crates/rustty-vt-c/src/terminal.rs:10691`, byte-identical to the two above and latent
  only because nothing happened to emit into its window (0 failures in 15 runs). Fixed the
  same way. Four further sites — `mouse_encode.rs`, `key_encode.rs`, `osc.rs`, `sgr.rs` —
  assert the WHOLE buffer equals an exact expected vector, which has the same exposure;
  each now filters to its own `LOG_SCOPE` first. Nothing else in the binary emits on those
  scopes, so no coverage is lost.
- **UNRESOLVED RESIDUAL, recorded rather than smoothed over**: during the fix round's
  repetition pass, ONE `cargo test -p rustty-lib -p rustty-apprt-api -p rustty-renderer -p
  rustty-vt -p rustty-vt-c` run failed with `error: test failed, to rerun pass -p
  rustty-renderer --lib` and no captured failure detail. It did **not** reproduce in 8
  further runs of the identical multi-crate invocation, nor in 18 runs of
  `cargo test -p rustty-renderer` alone. `rustty-renderer`'s `log_capture` module documents
  a known hole of exactly this shape in its own header — a capture observes only records
  emitted on the thread that called `start()`, and the crate contains genuinely
  multi-threaded code (the renderer thread, the CVDisplayLink callback) — and records that
  `rustty-vt`'s sibling module once failed 6 runs in 8 before admission control. Whether
  this one-off belongs to that family is UNKNOWN; it is written down here so the finale
  re-audit meets it as an item rather than as nothing.
- **state**: **CLOSED by task 15** for the three `rustty-vt-c` instances and the four
  exact-vector sites. The `rustty-renderer` one-off above is **OPEN as an observation**.

## N29 — the `checkGhosttyHEnum` ports bound header ↔ a hand-typed literal, and never reached the Rust enums

- **id**: N29 (next free id checked 2026-09-10 against `NEW-FINDINGS.md` headings and a
  workspace grep for `\bN[0-9]+\b`: N1-N28 taken, N26-N28 by this same task.)
- **severity**: **high** (22 C-ABI-sync tests that could not observe the thing they name;
  the same defect class as `S45-F1`, one link further along the chain)
- **kind**: weakened-assertion / verification instrument
- **rustty**: `crates/rustty-apprt-api/src/action_tests.rs`,
  `crates/rustty-apprt-api/src/ipc.rs`, `crates/rustty-renderer/src/lib.rs`,
  `crates/rustty-vt/src/osc/mod.rs`, `crates/rustty-vt/src/mouse.rs`.
- **description**: `S45-F1`'s fix made the `ghostty_h_*` tests read the shipped C header
  instead of the descriptor under test. But the descriptor was still built by
  `enum_type(Target::C, &keys)` — the `lib.Enum` **factory** model, which assigns each key
  its INDEX in the key list. The key list is a hand-typed literal. So the chain was:

  ```
  header  <->  position in a hand-typed literal        (this test)
  header  <->  reference                                (check_rustty_h_abi)
  literal <->  the port's actual Rust enum              NOTHING
  ```

  The `let _ = (ActionKey::Quit, ..)` line at the top of each test is a token touch for
  `refAllDeclsRecursive` reach — it names the enum, it does not compare anything to it.

  **Proven by the reviewer**: transposing `NewWindow` and `NewTab` in
  `crates/rustty-apprt-api/src/action.rs:412` — a live `#[repr(i32)]` enum whose own doc
  says "Sync with: rustty_action_tag_e", so a genuine C-ABI tag break — left **27 tests
  passing and all seven binary-interface gates green**.

  The reference has no such gap: `checkGhosttyHEnum(T, prefix)` reflects over `T`, so both
  the names and the values come from the enum itself. The factory model was simply the
  wrong model for a reflected enum — most `checkGhosttyHEnum` call sites pass a
  hand-written `enum(c_int)`, not a `lib.Enum` product.
- **fix**: new `rustty_lib::r#enum::enum_type_values(&[(name, value)])` — the reflected
  case — and every call site now supplies `<RustEnum>::Variant as i64` for the value half
  (`ProgressReportState::int_from_enum()` for the one type whose C value is a hand-written
  match rather than a `repr` discriminant, which is the function `cval()` ships). In
  `action_tests.rs` a `henum!` macro writes the pairing once. All 22 tests now compare the
  header against the enum's real discriminants.
- **negative control**: the reviewer's exact transposition now fails with
  `EnumType key new_window does not have the same backing int as RUSTTY_ACTION_NEW_WINDOW`;
  restored, 27/27 green.
- **what is still hand-typed**: which VARIANT a given snake_case key name maps to
  (`("quit", ActionKey::Quit)`). A rename of a Rust variant is a compile error, and a
  reorder, an inserted variant or an explicit discriminant all now fail the test — so the
  residual is only a mis-typed pairing at authoring time, which the header comparison
  catches for every key whose value differs.
- **state**: **CLOSED by task 15 fix round 1.**

## N30 — `snapshot/grid.zig`'s `Remap.init` is fallible upstream and INFALLIBLE in the port, and no ALLOC row claims it

- **id**: N30
- **severity**: low (the reference's `Allocator.Error` path is unreachable in the port; on
  real OOM the port aborts where the reference returns `error.OutOfMemory` and the whole
  snapshot decode degrades)
- **kind**: missing-error-path (the ALLOC family)
- **where**: reference `src/terminal/snapshot/grid.zig:1523-1532`
  (`fn Remap(comptime Id) type { pub fn init(alloc: Allocator) Allocator.Error!Self }` —
  `try alloc.alloc(Id, capacity)` plus `try std.DynamicBitSetUnmanaged.initEmpty`);
  port `crates/rustty-vt/src/snapshot/grid.rs:1833` `pub fn init() -> Remap`, whose
  `vec![0u16; CAPACITY]` / `vec![0u64; CAPACITY / 64]` abort instead of erroring.
  Reached from `snapshot/page.rs`'s `decode_payload_body` (two sites) whenever a decoded
  page declares a non-empty style or hyperlink table; `PayloadDecodeError::OutOfMemory`
  already exists there, so the error has somewhere to go.
- **CORRECTION (fix round 1, 2026-09-11): a row DOES claim this — `S204-F1`.** Filing this as
  unclaimed was wrong, and the way it was wrong matters: `S204-F1` cites the port side
  (`snapshot/page.rs:457, 561-573`) rather than `grid.zig`, so a grep of the ledger's ORACLE
  column for `grid.zig` found nothing and the row was invisible to the check that was run.
  Worse, `S204-F1` stood **WAIVED** over this exact allocation ("the remap initialisers are
  infallible, matching the crate-wide convention") while this entry called the same code a
  defect — a self-contradiction inside one task's output, caught in review. `S204-F1` has been
  re-opened (`ADJUDICATION.md#s204-f1`) and now OWNS this finding. **Lesson: when checking
  whether a row claims a reference site, search the ledger's RUSTTY column too — a row is
  allowed to be anchored on either side.**
- **why it is filed rather than folded into S206-F1**: `S206-F1` cites
  `src/terminal/snapshot/snapshot.zig:1626-1694` — the transactionality TEST — and task 16
  restored that test's shape and instrumented the decode allocation sites the row names
  (`PROCEDURE.md` blind-spot 1 carve-out: the leftover falls outside the row's cited oracle
  anchors, so it gets its own tracked entry instead of living in a closed row's footnote).
  `Remap.init` is in a different reference file that no ALLOC row cites. The complete
  golden fixture's pages declare empty style/hyperlink tables, so they take the `empty()`
  branch and allocate nothing — the sweep's count is unaffected either way, which is
  exactly why this could sit unnoticed.
- **fix**: make `Remap::init` return `Result<Remap, OutOfMemory>` via `try_reserve_exact`,
  add the `snapshot::tw::SnapshotFp::Alloc` check in front of it, propagate at
  `decode_payload_body`'s two call sites, and raise the transactionality test's
  `min_allocations` floor with a fixture whose pages carry styles. 26 call sites, 24 of
  them in `grid.rs`'s own tests.
- **state**: **CLOSED** (triage T-1, 2026-09-28): fixed with `S204-F1` by task T-A (`0bc065f8`; `S204-F1` CLOSED in `LEDGER.tsv`), as the update below records. Was: **OPEN**, tracked by **S204-F1** (re-opened 2026-09-11). Not attempted by task 16.
- **update (batch T-A)**: FIXED with S204-F1. `Remap::init` returns `Result` through
  `try_reserve_exact` behind a `SnapshotFp::Alloc` check, `decode_payload_body` returns
  `OutOfMemory` for either remap, and the transactionality sweep has a styled fixture and
  raised floors (hyperlink fixture 13 to 14, styled fixture 10).

## N31 — the Miri runner fabricates failures: it classifies on an abort STRING, and it uses a fixed temp path two sessions collide on

- **id**: N31
- **severity**: medium (no product code is wrong; the harm is to every future reader of the
  memory-safety lane, which is one of the few gates that can catch real UB)
- **kind**: instrument defect (the "verification instrument that cannot fail" class, inverted —
  this one *fails when nothing is wrong*)
- **where**: `.port/tools/miri-run.sh`
- **two independent defects, both observed, not reasoned:**

  1. **It classifies a run by grepping the captured output for an abort string, and that string
     is what the compiler prints for ANY compile failure.** So a build break reads as
     `res=UB`. Observed by task 16: a first `snapshot::` sweep reported **`ub=5`**, and all five
     output files begin `error[E0603]: trait import 'Allocator' is private` — a stale edit, not
     undefined behaviour. Observed again on
     `tabstops_resize_reports_a_real_allocation_failure_without_the_tripwire`, which reported
     `res=UB` for `error: resource exhaustion: tried to allocate more memory than available to
     compiler` — Miri does not MODEL allocation failure, it hard-errors the interpreter, so a
     test that deliberately fails an allocation can never pass under Miri and is not UB.
  2. **It uses a FIXED temporary directory (`/tmp/mirirun.$CRATE`), which it `rm -rf`s at
     startup.** A second session running the harness on the same crate deletes the first run's
     in-flight output. Found in review: a concurrent run wiped the captured files and the
     harness **reported three failures for tests that had exited cleanly**. That is the worst
     failure mode an instrument can have — not a missed defect but a manufactured one, and a
     manufactured one costs the next person a bisect.

  A third, smaller thing worth knowing: the script's "building Miri test binary once" line is
  misleading — each worker re-invokes `cargo miri test`, so editing the tree during a run
  rebuilds mid-flight. That is how defect 1 was first triggered.

- **suggested fix (all three, together — a gate change must buy net-new coverage under R5, and
  this does: today the lane cannot distinguish "UB" from "did not build")**:
  - classify on CONTENT, not on an abort string: a run is `UB` only if its output contains
    Miri's `Undefined Behavior` banner;
  - give compile failures (`error[E….]` / `could not compile`) and interpreter errors
    (`resource exhaustion`, `unsupported operation`) their OWN buckets, reported separately
    from `UB` and from `FAIL`. A `BUILD` bucket should abort the whole sweep, not be counted
    per test;
  - make the work directory unique (`mktemp -d`, or suffix the PID/run id), so two sessions
    cannot delete each other's evidence;
  - add a self-test arm per bucket — a fixture that fails to compile, one that hits a Miri
    interpreter limit, one with real UB — so the classifier is itself measured. Every
    instrument on this plan that was trusted without such an arm turned out to be wrong.
- **NOT fixed by task 16**: it is a gate (R5), and a gate change needs its own argument and
  self-test arms rather than a drive-by patch from the task it inconvenienced.
- **state**: **CLOSED** (G-1, 2026-09-28). `.port/tools/miri-run.sh`: (1) classifies on content (`classify`): UB only on Miri's `Undefined Behavior` banner, then LEAK (`error: memory leaked`), BUILD (`error[E....]` / `could not compile`), INTERP (`resource exhaustion` / `unsupported operation`), ok (`test result: ok. 1 passed`), TIMEOUT, FAIL; LEAK and FAIL are problems, INTERP needs review, and a BUILD result aborts the sweep (verdict BUILD-BROKEN, exit 1; a PROBLEM verdict now exits 1 too); (2) the work dir is `mktemp -d` per run and printed; (3) the "building once" line now says it only prebuilds and that workers re-invoke `cargo miri test`; (4) `--self-test` classifies real Miri outputs captured into `.port/tools/miri-run-fixtures/` (source `fixture_lib.rs.txt`; nightly miri 0.1.0 `6f72b5dd5f`): pass, use-after-free, leak, resource exhaustion, unsupported operation, compile failure, ordinary failure, plus a TIMEOUT arm and a distinct-work-dir arm; registered as `G-MIRIRUN-SELF` (HARD) in `.port/tools/ci.sh`. Observed on the fixtures: a leaking test ALSO prints `1 passed`. Negative controls: the old abort-string rule restored → self-test FAIL on leaks/exhausts/unsupported (all → UB); a fixed `/tmp/mirirun.$1` work dir → FAIL "work dirs ... ...". End-to-end: `miri-run.sh rustty-zigstd wyhash 2 5 5` → 7/7 ok, VERDICT ALL-7-MIRI-CLEAN. Was: **OPEN** (triage T-1, 2026-09-28). Unchanged: `.port/tools/miri-run.sh:23` still uses the fixed `/tmp/mirirun.$CRATE` it deletes at startup, and `:62` still classifies UB on `Undefined Behavior|memory leaked|error: aborting`. (`.port/tools/miri-sweep/driver.sh` classifies on the banner, but has no BUILD bucket or self-test.) Left: the four-part fix written above. Effort S, no long run; internal tooling only. Was: **OPEN**.

---

## N32 — `winproto/noop.zig`'s `updateConfigEvent` cannot be ported: its parameter type does not exist at the pin

- **id**: N32 (next free id checked 2026-09-11 against this file's headings; N31 was the last).
- **found by**: task-18, closing `S60-F9`.
- **class**: DOC / unportable reference decl.

`S60-F9`'s stale PORT NOTE in `crates/rustty-apprt/src/gtk/winproto/noop.rs` claimed
`eventMods`, `initQuickTerminal` and "the `Window`-parameterized methods" were unported. All of
those are live (`:55`, `:79`, and the whole of `noop::Window`), so the row closes. But the note
was not 100% false, and the residue is worth its own entry rather than a footnote inside a
closed row (`PROCEDURE.md` blind spot 1, the carve-out):

`noop.zig:64-67` declares

```zig
pub fn updateConfigEvent(_: *Window, _: *const ApprtWindow.DerivedConfig) !void {}
```

and **`ApprtWindow.DerivedConfig` does not exist at pin `20abdb50a`.** `grep -rn "DerivedConfig"
/Users/gael/projects/ghostty/src/apprt/gtk/class/window.zig` returns nothing, and the only hit
for `updateConfigEvent` anywhere in the reference tree is this one declaration. Zig accepts it
because nothing calls it, so the signature is never analysed. The `x11` and `wayland` arms of
the same `winproto.Window` union do not declare it either, so the union never dispatches to it.

**There is nothing to port it against**, and porting it would mean inventing a `DerivedConfig`
the reference does not have — which is worse than the gap. The exception is recorded in the
three tracked places the carve-out requires: this entry, `S60-F9`'s `owner` cell, and a PORT
NOTE at `noop.rs`.

**The generalisable point.** A reference decl can be *unreachable* in a way that makes it
unportable, and Zig's lazy analysis is what lets that state persist upstream. Rust has no
equivalent — a Rust signature naming a missing type does not compile — so "the reference has it
and we don't" is not automatically a defect. Check that the reference's own version would
compile if anything called it.

**state**: **STANDING (watch item, not a defect)** (triage T-1, 2026-09-28): nothing exists at the pin to port it against (`ApprtWindow.DerivedConfig` is absent and nothing calls `updateConfigEvent`); recorded upstream as UD29 and under `PROCEDURE.md`'s carve-out (ruled 2026-09-09). Reopen only if a later pin gives it a caller. Was: OPEN as a watch item. Discharged if upstream gives `updateConfigEvent` a caller or a
real `DerivedConfig`; nothing to do until then.

**upstream record**: `UPSTREAM-DEFECTS.md` UD29 (task R13-R).

---

## N33 — the stale-note class HAS a mechanically checkable subset: module-header test-count totality claims (4 unclaimed instances found)

- **id**: N33.
- **found by**: task-18, answering its brief's research question.
- **class**: DOC / gate design. **Not built as a gate** — task-18 was not scoped for one.

> **IF YOU ARE HERE TO BUILD THE NAME CHECK, DON'T.** "Flag a note naming a file, symbol or
> crate that does not exist" is the intuitive check in this class and it was measured: **6 hits
> tree-wide, at most 1 real, 0 of the 8 DOC rows.** The failure is **structural, not a regex
> problem** — see Candidate A. Port comments quote three namespaces in ONE indistinguishable
> syntax: Rust crates, *reference Zig identifiers that R1 requires be reproduced and renamed*
> (so they look exactly like port names and are not), and prose names for plans and docs.
> Nothing in the token tells them apart, the R1 class is the largest of the three, and a
> checker here would **punish fidelity**: the single largest false-positive family is
> `rustty-build-data`, a correct R1 rename of the reference's own `ghostty-build-data` comment.
>
> **And do not build the "deferred beside live code" check either.** On `S60-F9` it would have
> fired on a note whose residual claim is **TRUE** (`updateConfigEvent`, N32) and demanded the
> note be deleted — **destroying a true statement**. A check that cannot represent "half
> false" will launder half-false notes into no notes at all, which is strictly worse than the
> defect. See Candidate C.

Task-18's brief asked whether the recurring stale-note class can be checked by machine. It can,
for one narrow subset, and the measurement is below. Three candidates were probed against the
tree at `HEAD`; scripts are transient, the numbers are reproducible from the descriptions.

### Candidate A — "a note naming a file, symbol or crate that does not exist": NO

Scan every `//` comment in `crates/**` and `xtask/**` for a backticked `` `rustty-*` `` token
and flag the ones that are not a directory under `crates/`. **6 hits, at most 1 real, 0 of the
8 DOC rows.** The five false positives are all legitimate: `rustty-completion-plan` (a memory
doc), `rustty-gen` (a hypothetical crate named in a rationale for NOT creating it),
`rustty-integration` (a shell-integration *file*), `rustty-ssh-` (a terminfo entry prefix), and
`rustty-build-data` (an R1-faithful rename of the reference's own `ghostty-build-data` comment
at `Config.zig:48` — a Zig binary, not a Rust crate). The one arguable hit is
`rustty-build-config/src/build.rs:11`'s "`rustty-xtask` build tooling", where the crate is
called `xtask` — a naming slip, not a stale note about port state.

**Why it fails, and the failure is structural, not fixable by a better regex.** Port comments
quote three namespaces that look identical: Rust crates, *reference* Zig identifiers (which R1
requires be reproduced and renamed, so they look like port names and are not), and prose names
for plans and docs. A checker cannot tell them apart from the token, and the R1 class — the one
fidelity demands the most of — is the largest.

### Candidate B — "a note asserting a count that can be counted": YES, narrowly

Broad form (any `N … tests?` in any comment of a file whose header names one oracle `.zig`,
compared against the oracle's `^test` count and the port's `#[test]` count): **143 claims
examined, 74 flagged.** Unusable — most are sub-counts ("the 2 darwin-only and 4 windows-only
tests", per-module breakdowns in a crate root, "the 8 tests that exercise them"), which are
correct statements about a subset and have no reason to equal either total.

Narrowed form — a **totality** claim (`all N tests`, ``all N `test{}` blocks``,
``(N `test{}` blocks``) in the **first 30 lines** of a file whose header names exactly one
oracle `.zig`, excluding any line containing `~`: **23 claims examined, 7 flagged, 5 true.**
Precision 71%, and it caught the audit's own `S139-F1` plus **four instances no row claims**:

| site | claimed | oracle `test` blocks | port `#[test]` | verdict |
|---|---|---|---|---|
| `rustty-vt/src/selection.rs:8` | 18 | 20 | 20 | TRUE (this is `S139-F1`) |
| `rustty-vt/src/render.rs:5` | 15 | 20 | 23 | TRUE, unclaimed |
| `rustty-vt/src/stream.rs:7,21` | 38 | 64 | 75 | TRUE, unclaimed |
| `rustty-renderer/src/link.rs:7` | 3 | 4 | 5 | TRUE, unclaimed |
| `rustty-termio/src/stream_handler.rs:4` | 0 | 2 | 11 | TRUE, unclaimed |
| `rustty-vt/src/tmux/control.rs:14,18` | 8 | 18 | 24 | FALSE POSITIVE — a correct subset count (the 8 Oniguruma-gated tests) |

All four unclaimed ones were checked test-name by test-name before any prose was touched: in
every case the WORK was done and only the number was stale, so correcting the header removes a
lie rather than moving one forward. All four are fixed in task-18's commit with an inline
CORRECTION pointing here.

**What it would cost.** ~80 lines of Python reusing `check_log_reconcile.py`'s existing
oracle-tree walk, plus a declaration file for the subset-count false positives (the `tmux`
shape), ratcheted the way that gate ratchets allowances. **What it would miss:** every stale
note that is not a number — which is 7 of this task's 8 DOC rows.

**Two measurement traps this probe fell into itself**, both instances of `PROCEDURE.md`'s
"count through the instrument" rule:

1. Counting oracle tests with `^test\s+["{]` misses Zig's **identifier form** (`test detectShell
   {`, `test Action {`). That under-count manufactured a defect in
   `termio/shell_integration.rs` (claim 21, "oracle 20") that does not exist — the reference
   really has 21. The correct pattern is `^test\b`.
2. Counting ported tests by preserved `// test "…"` comments under-counts, because a port may
   record the reference test name in a `///` doc comment instead. That manufactured a missing
   test in `render.rs` which is in fact ported at `:2593`.

A count check whose own counter is wrong produces exactly the artifact it exists to catch.

### Candidate C — "not ported / deferred / placeholder / no-op next to code that is none of those": NO as a gate, YES as a lint with a human

This is the shape of 5 of the 8 DOC rows, and it is where the class actually lives. It cannot
be decided mechanically, for a reason visible in this task's own rows:

- **`S208-F3` was half true.** The note said the `oni` binding crate "does not exist yet, so the
  gated code is NOT compiled in the default build". The crate exists (`rustty-oniguruma`) — but
  the feature is still default-OFF, so the *conclusion* remained true. A checker keying on
  "`#[cfg(feature = ...)]` exists" would have called it fine; one keying on "the named crate
  exists" would have called it stale and been right for the wrong reason. The stale part was the
  REASON, and a reason is not a predicate.
- **`S211-F7`'s header listed nine deferred functions, all live.** That one IS mechanically
  catchable: "a name in a DEFERRED sentence that resolves to a live `fn` in the same file". But
  three per-function headers in that same file had already been struck individually (`[M3]`,
  `[WS20]`, `[W6]`) while the module header still summarised them as deferred — so what a
  checker would flag is the SUMMARY, and the summary is prose that names things loosely
  (`processExit*`, `ReadThread`).
- **`S60-F9` had a residue that is correct** (N32), and a checker firing on "note says unported,
  symbol exists" would have demanded the note be deleted outright, destroying a true statement.

### The rule

**A note is mechanically checkable exactly when it asserts a DENOTATION — a number, or a symbol
whose existence decides the claim. It is not checkable when it asserts a REASON, a SCOPE ("this
task lands only…"), or a PLAN ("ports with the GTK apprt in Phase-3").**

This is not an observation about eight rows; it is a rule for anyone WRITING a note, and it is
where the leverage is. A reason cannot be made checkable by a better checker — it can only be
made checkable by being written as a denotation in the first place. So the rule is stated in
`PROCEDURE.md` ("Writing a PORT NOTE"), where a future stage writing notes meets it, and
repeated here, where a future stage auditing them does. Reason-shaped notes are the majority
and the ones that go stale hardest, because the work they describe moves and the sentence does
not.

**A cheaper intervention than a gate, from the same evidence.** Every stale note found on this
plan was a claim about the FUTURE or about another part of the tree, written at the top of a
file, maintained by hand. `exec.rs` is the proof: a nine-name inventory in the module header
went stale nine ways at once, while the three per-function notes beside the actual code were
each struck as the code landed. **Deferral notes belong at the deferred thing, not in a summary
list at the top.** That is a convention, enforceable in review, and it would have prevented
`S211-F7`, `S72-F1` and `S83-F6` outright.

**state**: the four unclaimed instances are FIXED. The gate is NOT built — it needs its own
coverage argument and a self-test arm per `R5`, which is a separate task. This entry is the
finding.


---

## N34 — eight tmux control-mode notification arms are DEAD in the default build, because `oniguruma` is default-OFF and the reference compiles `oni` unconditionally

- **id**: N34 (next free id checked 2026-09-11 against this file's headings; N33 was the last).
- **found by**: task-18, while closing `S208-F3`. **Filed separately on the coordinator's
  instruction so it does not depend on someone re-deriving it.**
- **class**: behavioural divergence (feature-gate default), NOT a DOC row.

### What `S208-F3` was, and what it was not

`S208-F3` was a DOC row: `crates/rustty-vt/src/tmux/control.rs`'s header claimed "the `oni`
binding crate does not exist yet … until the binding crate lands". The crate **does** exist
(`crates/rustty-oniguruma`, wired as `rustty-vt`'s optional `oniguruma` dependency,
`rustty-vt/Cargo.toml:113,221`), so the row closed. But that note was **half true**, and the
surviving half is a real divergence that no row claims.

### The divergence

The reference compiles `oni` **unconditionally**: `tmux/control.zig` does
`const oni = @import("oniguruma")` with no build option, so every notification arm is live in
every ghostty build. In rustty the `oniguruma` cargo feature is **default-OFF**, so in the
default build these eight arms fall through to the "unknown notification" path:

`%output`, `%session-changed`, `%layout-change`, `%window-add`, `%window-renamed`,
`%window-pane-changed`, `%client-detached`, `%client-session-changed`

and the 8 reference tests that exercise them are not compiled. Counted and verified at pin
`20abdb50a`: `control.zig` has 18 `test` blocks, 8 of them behind the gate, 10 (the begin/end
blocks and `%sessions-changed`) live by default. `%output` is the arm that carries tmux
control-mode's actual pane output, so what is dead by default is not an edge case.

### Why this is not simply "turn the feature on"

It is a real decision with a cost, which is exactly why it needs a row rather than a drive-by:

- `rustty-oniguruma` is an FFI binding over a C library, and the port already carries a
  `check_oniguruma_lane.sh` gate (G-ONIG) because that lane is otherwise linted by nothing.
  Making it default-on moves every default build onto that dependency.
- The same feature gates `string_map.rs`'s regex search surface and `renderer/link.rs`'s
  `oni::Regex` compilation, so the blast radius is wider than tmux.
- The reference's own build has no equivalent switch, so **there is no upstream behaviour to
  match for the OFF state** — the OFF state is a rustty invention, and every hour it stays the
  default is an hour the shipped default diverges from ghostty.

### What task-18 did and did not do

**Did:** corrected the note so it states the true part precisely (feature default-OFF ⇒ arms
not compiled) instead of justifying it with a false premise (crate does not exist). The counts
in that header were re-counted and are correct.

**Did not:** decide the feature default. That is out of scope for a DOC row, and closing
`S208-F3` on a prose fix must not be read as retiring the divergence — which is precisely the
failure mode this file exists to prevent.

**state**: **STANDING (not a defect: the premise is refuted at the pin)** (triage T-1, 2026-09-28): the reference does NOT compile `oni` unconditionally. `src/terminal/build_options.zig:296` sets `tmux_control_mode = oniguruma` and `src/terminal/main.zig:30` compiles `tmux` only under it; the app's `terminalOptions` sets `.oniguruma = true` (`src/build/Config.zig:714`) and lib-vt sets it false (`src/build/GhosttyZig.zig:72`). The port has the same split: `rustty-vt`'s feature is default-off (the lib-vt build) and `rustty-core` turns it on (`crates/rustty-core/Cargo.toml:69`), so the app has all eight arms live, and G-ONIG compiles and tests the feature lane in `ci.sh`. No decision is owed. Was: OPEN. Needs a decision on the `oniguruma` feature default, and — if it stays OFF — a
recorded justification for shipping with eight notification arms and `%output` dead, sitting
next to the gate that lints the lane nobody compiles by default.

## N35

**`search::thread::tests::search_thread` is LOAD-DEPENDENT** — a real flake, filed by task-17
fix round 1 on the task-17 reviewer's measurement.

**Measured by the reviewer, not by me**: clean in 25 isolated runs; **one failure in six** when
run under contention with a concurrent memory-safety (Miri/ASan) sweep on the same host. It is
not in task-17's diff and no change of mine touches `search/thread.rs`, so it is not a
regression from this task — but it does mean **every clean repetition result reported on this
plan describes the tests the task touched, not the suite**. Three consecutive green
`cargo test --workspace` runs on an idle machine cannot see this.

Why it matters beyond the flake itself: this is the first evidence on this plan that the
workspace suite's greenness is a function of machine load. A CI leg that runs the suite beside
anything else — and the Miri/ASan lanes do exactly that — can go red for reasons that have
nothing to do with the change under test, which is the fastest known way to teach a team to
re-run a red build instead of reading it.

`search_thread` drives a real OS thread plus a `Mutex`/condvar handshake with the search
coordinator, so a scheduling-sensitive assertion (a wait that assumes the worker has made
progress) is the obvious suspect. Deliberately NOT diagnosed here: naming the assertion needs
the failure reproduced under load, and guessing at it in this file is how a finding becomes
folklore.

**state**: **OPEN** (triage T-1, 2026-09-28). No later record reproduces or fixes it; `search/thread.rs` changed since (S7-6, S-1d) without addressing it. Left: reproduce under deliberate load, name the failing assertion, decide test versus handshake, and measure repetitions under load. Effort M, and it needs a long run (repetitions under load), which the wrap-up defers; internal test reliability unless the handshake itself is wrong (then observable as a stalled search). Was: OPEN. Needs (1) the failure reproduced under deliberate load and the failing
assertion named, (2) a decision on whether it is the test or the handshake that is wrong, and
(3) if the suite is to be trusted under contention, a repetition measurement that is taken
under load rather than on an idle host.


## N36 — the header-sync tests are HALF-blind: 22 `checkGhosttyHEnum` ports cannot see a variant ADDED to the Rust enum

Found by task 19 while measuring `S47-F1` and `S156-F1`, both of which name the defect
`S45-F1`/`N29` already fixed. Those fixes are real and I confirmed them by measurement
(transposing two `ActionKey` variants turns `ghostty_h_action_key` RED). This is the
**remaining half of the same seam**, and no row claims it.

**The reference reflects; the port writes a list.** `lib.checkGhosttyHEnum` iterates
`@typeInfo(T).@"enum".fields`, so every variant the enum HAS is checked against the header by
construction. Rust has no such reflection, so
every one of this workspace's five `check_ghostty_henum` callers supplies a HAND-TYPED key list.
`N29` bound the values (`<Enum>::Variant as i64`); nothing bound the roster.

**Measured, not argued.** Appending a variant to the end of an enum (the realistic case — a new
action goes on the end) with no header enumerator and no entry in the test's list:

| site | enums | before | after |
|---|---|---|---|
| `rustty-apprt-api/src/action_tests.rs` | 17 | **27 tests GREEN** | compile error |
| `rustty-apprt-api/src/ipc.rs` | `ActionKey`, `TargetKey` | **3 tests GREEN** | compile error |
| `rustty-renderer/src/lib.rs` | `Health` | **2 tests GREEN** | compile error |
| `rustty-vt/src/mouse.rs` | `Shape` | **6 tests GREEN** | compile error |
| `rustty-vt/src/osc/mod.rs` | `ProgressReportState` | already a compile error | unchanged |

Note the shape of the blind spot: INSERTING a variant in the middle shifts the discriminants and
is caught by the value comparison `N29` restored. Only an APPENDED variant is invisible — which
is the one people actually write. `mouse.rs`, the one caller task 15 recorded as having "did it
right", had the gap too.

`ProgressReportState` was already bound, by its own `int_from_enum()` exhaustive `match` — not by
anything the test does. That is luck, not design, and it is why the fix is a shared device rather
than a per-site argument.

CORRECTION (task 19 fix round 1, reviewer): this paragraph used to say the reference binds
"BOTH directions … a header enumerator that the enum does not have, and an enum variant the
header does not declare". **It does not.** `checkGhosttyHEnum` iterates only the enum's fields
(`src/lib/enum.zig:126`) and removes matched values from an `EnumSet(T)`; a header enumerator the
enum lacks is caught by NEITHER side, and the A3 header gate checks function symbols only. The
port is faithful on that direction. The blind spot this finding closes is the enum→header one,
which the reference does bind.

**Fix**: `rustty_lib::henum_type!` (and `rustty_lib::enum_name_table!` for the `TAG_NAMES`
class) expand the hand-written list into the checked `EnumType` AND an exhaustive `match` over
the enum, so a variant the list does not name stops the crate building. That is stronger than the
reference's failure mode (a test failure), and it is the closest Rust has to
`inline for (@typeInfo(T).@"enum".fields)`.

CORRECTION (task 19 fix round 1): the macro was first called `henum_fields!` and its doc claimed a
DUPLICATED list entry is caught as an unreachable-pattern lint under `-D warnings`. **That never
fires at a real call site**: rustc suppresses that lint for code expanded from another crate's
macro, so a duplicated `("unhealthy", Unhealthy)` in `rustty-renderer` passed test and clippy
alike (reviewer-measured). The round also found two more things the hand-written list could get
wrong silently, both of which the reference gets from reflection. All three are now observed:

| hand-written fact | reference | port before | port now | proof (real call site) |
|---|---|---|---|---|
| no row listed twice | reflection | lint suppressed cross-crate — GREEN | const-eval panic = compile error | duplicate `("unhealthy", Unhealthy)` in `rustty-renderer`; duplicate `(Key::Copy, "copy")` in `TAG_NAMES` → `a variant is listed twice` |
| key names its variant | key IS `@tagName` | transpose `Health`'s variants AND swap the list's keys → GREEN while `Health::Healthy` = 1 and `HEALTHY` = 0 | const-eval check, key vs `stringify!(Variant)` ignoring `_` and case | that two-edit mutation → `a key does not name its variant`; so does swapping `copy`/`cut` in `TAG_NAMES` |
| tag type is `c_int` | `info.tag_type == c_int` | `enum_type_values` hardcoded `CInt` — `#[repr(u8)]` on `Health` GREEN | `size_of` the enum (or the mapper's return type) | `#[repr(u8)]` on `Health` → `expected c_int tag_type`, both tests RED |

The key comparison ignores `_` and ASCII case because the Zig snake_case names are not a
mechanical function of the Rust idents (`digit_0`/`Digit0` but `f1`/`F1`, `osc8`/`Osc8`,
`export_terminal_io`/`ExportTerminalIO`); all 340 existing rows satisfy it. Residuals, by
construction: a size cannot tell `repr(i32)` from `repr(u32)`, and two variants whose names differ
only by `_` or case could be transposed undetected (Rust's naming lints make the latter
unrealistic).

**state**: CLOSED by task 19 at all five sites. Recorded here rather than reopening `S45-F1`,
whose own claim — "the C values are looked up in the enum itself" — was true, was fixed, and is
not this.

**upstream record**: the reference-side blind spot of `checkGhosttyHEnum` is
`UPSTREAM-DEFECTS.md` UD16 (task R13-R).

## N37 — `xtask dist-lib-vt` has been BROKEN since task 17, and G-DIST runs it at its DEFAULT depth

Found by task 19 the moment `distcheck-lib-vt` (S63-F7) existed to run it.

`prune_to_lib_vt` computes the lib-vt tarball's crate closure from `cargo metadata` and
deliberately skipped path DEV-dependencies, with the comment *"Dev-deps are skipped: they pull in
test-only crates a source tarball for C consumers does not need."*

**That premise is wrong twice.**

1. **Cargo cannot LOAD a workspace whose member declares a path dev-dependency that is not on
   disk.** It fails at `cargo vendor`, during `dist` itself, with
   `failed to load manifest for dependency 'rustty-synthetic'`. The tarball is never produced.
2. **The reference's check step for this exact tarball RUNS THE TESTS in it**
   (`zig build test-lib-vt -Demit-lib-vt=true`), so the test-only crates are precisely what it
   needs.

**Measured at this task's base commit** `5a73f0bc`, in a clean worktree, with nothing of mine
applied: `cargo run -p xtask -- dist-lib-vt` fails. The breaking change is task 17's S155-F5,
which gave rustty-vt a `rustty-synthetic` dev-dependency — a change whose own report flagged the
dev-dependency as novel (§11.5) and still could not see this, because nothing ran the command.

**What it says about the gate.** `check_dist.sh` runs `xtask dist-lib-vt` at its DEFAULT depth,
not behind `RUSTTY_DISTCHECK=1`. So G-DIST has been RED — or unrun — for the whole interval. A
gate that nobody runs is indistinguishable from a gate that does not exist, and this one was
reporting on the artifact `release-tip.yml` publishes as `librustty-vt-source.tar.gz`.

**Fix**: dev-deps are closure edges (`xtask/src/dist.rs`, `prune_to_lib_vt`), and the false
comment is replaced by a CORRECTION quoting it. `xtask dist-lib-vt` and the new
`xtask distcheck-lib-vt` both pass.

**state**: CLOSED by task 19. Recorded because the *class* is open: a gate step whose failure
nobody observes. Worth asking, at the finale, which G-* steps have actually run since they were
written.

## N38 — an unclaimed port panic in the GlobalShortcuts `Activated` handler, reachable from any session-bus peer

Found by task 20 while fixing S51-F7. No row claims it.

`global_shortcuts.rs`'s `Activated` subscription read the shortcut id with
`signal.parameters.child_value(1)`. glib-rs's `child_value` ASSERTS `index < n_children()`, so a
`(o)` body — which passes the subscription's `MATCH_ARG0_PATH` filter when it carries the session
handle, and which any process on the session bus can send because the sender is null — panicked
inside the D-Bus dispatch on the GTK main loop. The reference is no better here
(`UPSTREAM-DEFECTS.md` UD2): the same `(o)` body makes GLib abort in `g_variant_get_child`, and a
wrong-typed `(oo)` body leaves `shortcut_id` undefined and reads it. There is no reference
behaviour to port. (Fix round 1: this said only that the reference "reads an undefined
pointer", which is the `(oo)` case alone.)

Under R13 the port now reads the child only if it exists and is type `s`
(`activated_shortcut_id`), and returns otherwise. Test:
`global_shortcuts_activated_shortcut_id_is_safe_on_malformed_bodies`; negative control (restore
`child_value(1)`) RED with glib's `variant.rs:314` index assertion, measured on the VM.

The same class, both sites claimed or recorded: the `Response` half is S51-F7 + UD1, and
OpenURI's `responseReceived` (S59-F3) was the same `child_value(0)` panic, removed by porting the
reference's `params.get` exactly.

CORRECTION (task 20, before commit): this entry was first drafted as "the REFERENCE panics ... the
port keeps the panic on purpose", recording a controller ruling to port `vardict.?` as a panic. The
user overruled that ruling the same day (R13); the reference defects moved to
`UPSTREAM-DEFECTS.md` UD1/UD2 and this entry now records only the port-side site.

**state**: CLOSED by task 20 (`d9de0339`).

## N39 — a tmux control-mode block over 1 MiB left the viewer alive, and the next attach panicked

- **id**: N39 (next free id checked 2026-09-11 against this file's headings; N38 was the last).
- **found by**: the R13 inventory (code sweep T1-1, `r13-inventory.md` M1); fixed by task R13-A.
- **severity**: high. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/terminal/dcs.zig:114-121`, `src/termio/stream_handler.zig:410`.
- **rustty**: `crates/rustty-vt/src/dcs.rs:204` (`Handler::put`),
  `crates/rustty-termio/src/stream_handler.rs:539` (the `.enter` assert).

The port copied the reference: a discarded tmux DCS state emitted no `.exit`, so the viewer
survived and the next `ESC P 1000 p` panicked at `stream_handler.rs:538`. No row claimed it.
Under R13 `put` now emits `.exit` when it discards a tmux state. The reference defect is
`UPSTREAM-DEFECTS.md` UD6, with the tests and the negative control.

**state**: CLOSED by task R13-A (`e9808b18`).

## N40 — eight tmux notification id captures unwrapped an overflowing `parseInt`

- **id**: N40.
- **found by**: the R13 inventory (M2; code T1-2, records F2, sessions B1, phase-3 U3); fixed by
  task R13-A.
- **severity**: high. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/terminal/tmux/control.zig:238,269,309,347,377,408,413,470`.
- **rustty**: `crates/rustty-vt/src/tmux/control.rs:970` (`parse_capture_usize`) and its eight
  call sites.

`.port/phase-3/UPSTREAM_BUGS.md` U3 recorded these as "left faithful" (a 20-digit id panicked the
port as it does the reference). No LEDGER row. Under R13 an overflowing id takes the arm's own
match-failure path. Only live with the `oniguruma` feature, which the app enables (N34).
Reference defect: `UPSTREAM-DEFECTS.md` UD7.

**state**: CLOSED by task R13-A (`886fa0c7`).

## N41 — a zero tmux layout dimension crashed pane creation, and the pane-creation errdefer destroyed existing panes

- **id**: N41.
- **found by**: the R13 inventory (M4, code T1-3); the errdefer half by task R13-A while making the
  failure path reachable.
- **severity**: high (zero dimension); low (errdefer). **kind**: missing-branch (reference
  defect, R13) + port divergence on an error path.
- **oracle**: `src/terminal/tmux/viewer.zig:1160-1161` (zero passes `@intCast`), then
  `src/terminal/page.zig:274` / `src/terminal/PageList.zig:647`; the errdefer at
  `src/terminal/tmux/viewer.zig:635-644`. (Fix round 1: this cited `:365-374`.)
- **rustty**: `crates/rustty-vt/src/tmux/viewer.rs:1006` (`sync_layouts`).

S209-F1 covers only the out-of-range half of this site. A zero width or height passed the port's
checked cast and panicked inside `Terminal::init` — measured: `page_impl.rs:450` (index out of
bounds) for a zero width, `pagelist/mod.rs:1928` (`!page_list.first.is_null()`) for a zero height.
The inventory's other candidate, `terminal.rs:988-990` (`rows - 1`), is never reached first.
Under R13 a zero dimension is a failed `Terminal::init`, like an out-of-range one.

That made the `Terminal::init` failure branch reachable from input, and it diverged from the
reference: it deinited every pane in the new map, including panes carried over from
`self.panes`, where the reference's errdefer deinits only panes that are not in `self.panes`.
The carried-over panes are now moved back. The viewer is defunct either way, so the visible
difference was only which panes a defunct viewer still held. Test assertion: "pane 7 survives, as
in the reference's errdefer". Reference defect: `UPSTREAM-DEFECTS.md` UD8.

**state**: CLOSED by task R13-A (`886fa0c7`).

## N42 — tmux layout parsing and teardown recursed without limit on a 2 MiB stack

- **id**: N42.
- **found by**: the R13 inventory (M6 code T1-4, P9); measured and fixed by task R13-A.
- **severity**: medium. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/terminal/tmux/layout.zig:87-230` (recursive call at :180).
- **rustty**: `crates/rustty-vt/src/tmux/layout.rs:157` (`parse_next`), the `Drop for Layout`
  above `ParseError`.

The port recursed like the reference, and on a smaller stack: measured on macOS, the parser
overflowed at about 1,500 levels in a debug build, and the derived `Debug` (used when the
termio action log formats a `.windows` action) at about 2,400 in a release build, against
about 9,200 / 39,000 for the reference on its 16 MiB thread. Under R13 nesting deeper than
`MAX_DEPTH` (1000) is a `SyntaxError` and `Drop` is iterative. Reference defect:
`UPSTREAM-DEFECTS.md` UD10. Linux aarch64 (VM): the port's walkers overflow at nearly the same
depths (parser about 1,480 debug / 6,270 release; `Debug` about 1,625 / 2,415), the reference at the
same 9,200 / 39,000, and the three tests pass run individually in debug and release. `MAX_DEPTH`
matches upstream tmux: commit `b7c5030004` (2026-09-09) added `LAYOUT_V1_MAX_DEPTH 1000` with the
same depth counting. Older tmux has no cap, so a hand-written `select-layout` string could nest
deeper, but split-built layouts cannot realistically reach 1000. (Fix round 1: this first said the
limit was not checked against tmux's source.)

**state**: CLOSED by task R13-A (`886fa0c7`).

## N43 — the tmux viewer's "active area should be empty" debug assert was reachable

- **id**: N43.
- **found by**: the R13 inventory (M21, code T1-9); trigger established by task R13-A.
- **severity**: low. **kind**: extra-check (reference defect, R13).
- **oracle**: `src/terminal/tmux/viewer.zig:1070-1077`.
- **rustty**: `crates/rustty-vt/src/tmux/viewer.rs:1787` (`received_pane_history`).

The port ran the check under `debug_assertions`. It panics when a pane's `list-panes` state (scroll
region, cursor below it) is applied before its history, which happens when a second pane is added
while the first pane's captures are queued. The inventory's trigger ("a forged capture-pane reply"
setting DECSTBM) does not work: an ESC in the reply ends the DCS. The check is removed, as in
ReleaseFast. Reference defect: `UPSTREAM-DEFECTS.md` UD9.

**state**: CLOSED by task R13-A (`886fa0c7`).

## N44 — a tmux `list-panes` scroll region outside the pane crashed the next line feed

- **id**: N44 (next free id checked 2026-09-11; N43 was the last). Inventory M111.
- **found by**: the task R13-A reviewer; fixed in the task R13-A follow-up.
- **severity**: medium. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/terminal/tmux/viewer.zig:1013-1024` (`receivedPaneState`, `scroll:` block).
- **rustty**: `crates/rustty-vt/src/tmux/viewer.rs:1665` (`received_pane_state`).

The port stored tmux's scroll region as the reference does. A bottom at or past the pane's rows
panicked the next line feed or wrap at `screen.rs:1577` (`cursor_down`); a one-row region away from
the top panicked at `screen.rs:2088` (`cursor_scroll_region_up`, `limit >= 1`). No row claimed
it. The port now ignores the region unless `top < bottom < rows`, as the same reply ignores bad
cursor and tab-stop geometry. Reference defect: `UPSTREAM-DEFECTS.md` UD11.

**state**: CLOSED by the task R13-A follow-up (`ac627fab`).

## N45 — a non-UTF-8 command-line argument panicked `Config::load_cli_args` on Linux and FreeBSD

- **id**: N45 (next free id checked 2026-09-11; N44 was the last). Inventory P1.
- **found by**: the R13 inventory's code sweep (P1, coordinator verified); fixed by task R13-B.
- **severity**: high. **kind**: behavioral-difference (the port's own defect; byte-slice fidelity).
- **oracle**: `src/config/Config.zig:4267` (`internal_os.xdg.parseTerminalExec(global.args().vector)`),
  `src/os/xdg.zig:105-118`.
- **rustty**: `crates/rustty-config/src/config_struct.rs:6697`, `crates/rustty-os/src/xdg.rs:175`.

The xdg-terminal-exec check built its argv with `std::env::args()`, which panics on the first
non-UTF-8 element. It ran before argv[0] was looked at, so any non-UTF-8 argument aborted startup
on Linux and FreeBSD (`rustty -e cat $'\xff'`, a Latin-1 path from a launcher). The reference reads
the raw byte argv. `parseTerminalExec` now takes byte slices, and the argv is read through
`args_os` and `arg_bytes`, the seam `args_iterator` already used. Not an R13 item: the reference
does not have this defect.

Test: `crates/rustty-config/tests/load_cli_args_non_utf8_argv.rs` (`harness = false`, because a
libtest binary cannot even start with a non-UTF-8 argv). It re-executes itself with argv[0]
`xdg-terminal-exec`, and with `rustty -e cat`, each followed by the argument `FF FE`, and checks the
command the child built. It passes on the Linux VM; on macOS the branch is not compiled and the
test does nothing. `parse_terminal_exec_keeps_non_utf8_bytes` (rustty-os) covers the parser.
Negative control, VM 2026-09-11: reinstating `std::env::args()` makes the child panic at
`library/std/src/env.rs:871` with `called Result::unwrap() on an Err value: "\xFF\xFE"`.

The first commit left `arg_raw.as_bytes()` in the Linux-only branch. The macOS build does not
compile that branch, and the first Linux build caught it (`63ad6e12`). Three other
`std::env::args()` calls remain in the workspace, all in developer tools rather than the app:
`crates/rustty-unicode/examples/dump_tables.rs:29`,
`crates/rustty-apprt-c/src/bin/translate_probe.rs:20` and `xtask/src/main.rs:122` (`xtask` is a
workspace member, `Cargo.toml:170`). Recounted by task R13-S, 2026-09-11: the earlier "two" missed
the xtask one. The `.port/phase-*/mirihar-*` Miri harnesses call it too, but they are not workspace
members.

**state**: CLOSED by task R13-B (`895cba09`, `63ad6e12`).

## N46 — a cell-relative `cursor-text` or selection colour with a custom shader panicked the renderer (S116-F1)

- **id**: N46.
- **found by**: parity audit S116-F1 (refuted on "same defined semantics", so no LEDGER row),
  `.port/phase-3/UPSTREAM_BUGS.md` U2, the R13 inventory (M5); fixed by task R13-B.
- **severity**: medium. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/renderer/generic.zig:2266-2293`.
- **rustty**: `crates/rustty-renderer/src/generic.rs:5688`,
  `crates/rustty-renderer/src/generic/linux_impl.rs:4127`.

S116-F1 found the port's `unreachable!()` in the three uniform writes, and the audit refuted it
because the port matched the reference. U2 recorded the same site as "ported as-is". Under R13 a
reachable crash is not ported: with any `custom-shader`, `cursor-text = cell-foreground` (or either
`*-invert-fg-bg` compatibility key) panicked both renderer twins. Each uniform is now written only
for an explicit colour and otherwise left unchanged, as when the option is unset. This entry is the
record for S116-F1; no LEDGER row was added. Reference defect: `UPSTREAM-DEFECTS.md` UD57.

**state**: CLOSED by task R13-B (`1ec776dc`).

## N47 — the initial window size and size-limit arithmetic panicked on overflow

- **id**: N47. Inventory M18.
- **found by**: the R13 inventory's code sweep (T1-6); `:705` confirmed by task R13-B.
- **severity**: low. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/Surface.zig:1856-1886`, `:705-706`.
- **rustty**: `crates/rustty-core/src/surface.rs:283` (`initial_size_px`, `size_limit_min`),
  `:2542` (`init`), `:5180` (`recomputeInitialSize`).

The port multiplied like the reference, and release keeps overflow checks, so `window-height =
1000000000` panicked at startup. It also converted the scaled size with a saturating `as u32` where
the reference traps. `:705` (the brief's unverified site) is reachable in arithmetic through
`adjust-cell-width`, not measured end to end. Both computations are now checked helpers: on
overflow `recomputeInitialSize` takes its existing skip path and `init` skips `size_limit`.
Reference defect: `UPSTREAM-DEFECTS.md` UD59.

**state**: CLOSED by task R13-B (`ff312abe`, test strengthened in `71255471`).

## N48 — a UTF-8 mouse report on a surrogate coordinate panicked

- **id**: N48. Inventory M22.
- **found by**: the R13 inventory's code sweep (T1-10); fixed by task R13-B.
- **severity**: low. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/input/mouse_encode.zig:134-149`.
- **rustty**: `crates/rustty-vt/src/input/mouse_encode.rs:245`.

The port's `char::from_u32(..).unwrap()` mirrored `utf8Encode(…) catch unreachable` and panicked
for a coordinate in 55263..=57310, reachable through the C-ABI mouse encoder. The event is now not
reported, and nothing is written. Reference defect: `UPSTREAM-DEFECTS.md` UD60.

**state**: CLOSED by task R13-B (`b802801b`).

## N49 — kitty shared-memory loads took SIGBUS on a truncated object, and formed a `&[u8]` over a peer-writable mapping

- **id**: N49. Inventory M20 and P6.
- **found by**: the R13 inventory (M20 code T1-8, unverified; P6 code); reproduced and fixed by
  task R13-B.
- **severity**: low (M20); low (P6, UB that no test can observe). **kind**: missing-branch
  (reference defect, R13) + unsafe-soundness (port only).
- **oracle**: `src/terminal/kitty/graphics_image.zig:245-273`.
- **rustty**: `crates/rustty-vt/src/kitty/graphics_image.rs:482` (`readSharedMemoryPosix`),
  `:579` (`pread_range`).

M20 was unverified with no reproducer. Measured on the Linux VM: an owner that shrinks the object
after the terminal's `fstat` makes the copy out of the mapping die with SIGBUS, both in a C probe of
the syscall sequence and in the port's own test through the VT stream. macOS cannot shrink POSIX
shared memory, and it cannot `pread` it either (ESPIPE), so macOS keeps the mapping. P6: the port
built `&[u8]` over the `MAP_SHARED` mapping, which the sender can write while it is borrowed. The
range is now read with `pread` (a short read is `InvalidData`), and the macOS path copies through a
raw pointer. Reference defect: `UPSTREAM-DEFECTS.md` UD61.

**state**: CLOSED by task R13-B (`7f93dbc8`).

## N50 — an OSC 7 path with `%00` aborted the app on the next new tab, split or window

- **id**: N50. R13-G part B, B1 (high) and B2.
- **found by**: the R13-G part B sweep (read, not run); verified and fixed by task R13-P.
- **severity**: high. **kind**: port-only panic on external input.
- **oracle**: `src/termio/Exec.zig:856-857` (`dupeZ`), `:868` (`env.put("PWD", pwd)`), `:981-985` (the
  `access` check and its `cannot access cwd, ignoring` warning); `src/apprt/embedded.zig:1167-1172`.
- **rustty**: `crates/rustty-termio/src/exec.rs` (`Subprocess.cwd`, `Subprocess::init`, the two `start`
  arms, `cstring`), `crates/rustty-apprt/src/embedded.rs` (`inherited_working_directory_c`).

A program that prints `ESC ] 7 ; file://localhost/tmp/a%00b BEL` plants a NUL in the terminal's pwd:
the port percent-decodes `%00` exactly as `std.Uri` does, `setPwd` stores it, and a new surface
inherits it as `working-directory` (`app.rs` `new_config`). `Subprocess::init` then built the cwd with
`cstring`, which `.expect`ed on the interior NUL — and the shipped macOS staticlib is `panic = abort`,
so the whole app died on the next tab. The same sink was reachable from a raw NUL byte in a config
`working-directory` value (B2), which the config line reader keeps.

`Subprocess.cwd` now holds the reference's raw bytes, and `start` takes the reference's own failure
arm: Zig 0.16's `access` converts the path with `pathToPosix`, which answers `error.BadPathName` for a
path holding a NUL (measured), logs `cannot access cwd, ignoring`, and spawns with no cwd. `PWD` still
carries the raw bytes, as the reference's does, and the child's environment is cut at the NUL by the
kernel (measured). Separately, the macOS inherited-config export returned `null` where the reference
hands the host a pointer into its `dupeZ` copy — the host's C string is the bytes before the NUL — so
that override was dropped entirely; it now matches.

**state**: CLOSED by task R13-P (`14284153`, `6d6cd3c1`).

## N51 — a NUL byte in `command`, `font-family`, a flatpak `env` value or host passwd output panicked

- **id**: N51. R13-G part B, B3, B4, B5 and B6.
- **found by**: the R13-G part B sweep (read, not run); verified and fixed by task R13-P.
- **severity**: low. **kind**: port-only panic on external input.
- **oracle**: `src/termio/Exec.zig:1847-2010` (`execCommand`'s per-argument `dupeZ`),
  `src/os/flatpak.zig:360-361`, `src/os/passwd.zig:121-122` (the `dupeZ` of the shell and home fields;
  first cited as `:86-88`, the `cmd.spawn`/`cmd.wait` lines, corrected by task R13-P2), and the fontconfig
  descriptor path.
- **rustty**: `crates/rustty-termio/src/exec.rs` (`cstring`), `crates/rustty-font-shaper/src/discovery.rs`
  (`to_fc_pattern`), `crates/rustty-os/src/flatpak.rs`, `crates/rustty-os/src/passwd.rs`.

Every one of these sites copies a config or host-supplied byte string into a C string. The reference
copies it with `dupeZ`, keeping the NUL, and hands the pointer to C — `execve` argv, `FcPatternAdd`, a
GVariant `{ss}` — which reads up to the first NUL. Measured with a Zig 0.16.0 probe: `/bin/sh -c` with
a NUL in the `-c` argument runs only the text before it, and `$0` loses everything after its own NUL.
The port panicked at each site instead. All four now cut the value at the first NUL, which is the value
the reference's consumers actually see. The passwd one is LEDGER S112-F5's third sub-defect; it is
hardening rather than a fixed crash, since glibc's own C-string formatting cannot emit a NUL into
`getent passwd` output.

**state**: CLOSED by task R13-P (`14284153`, `f019955e`).

## N52 — a C-ABI scroll event with momentum 7 aborted the library

- **id**: N52. R13-G part B, B7.
- **found by**: the R13-G part B sweep (which doubted the port's own comment); measured and fixed by
  task R13-P.
- **severity**: low. **kind**: port-only panic on external input.
- **oracle**: `src/apprt/embedded.zig:2128-2139`, `src/input/mouse.zig:62-70,89-100`.
- **rustty**: `crates/rustty-vt/src/input/mouse.rs` (`ScrollMods::from_int`).

`ghostty_surface_mouse_scroll` takes `scroll_mods` as a `c_int` and `@bitCast`s the low byte into a
packed struct whose `momentum` field is an exhaustive `enum(u3)`, so bit pattern 7 has no name. The
port's comment claimed the reference traps on it and reproduced that with `unreachable!()`. It does
not: measured in Debug, ReleaseSafe and ReleaseFast, `@bitCast` performs no tag check and carries the
value; only a `switch` on the field traps, and nothing in `src/` switches on `momentum` — the sole
reader is `scrollCallback`, which reads `precision`. The port now takes the field's own declared
default for the unnamed value; nothing reads it, so no behaviour depends on the choice.

**state**: CLOSED by task R13-P (`a809a6ae`).

## N53 — a font whose variation-axes read fails leaked its HarfBuzz font

- **id**: N53. R13-G part C, C1.
- **found by**: the R13-G part C sweep; fixed by task R13-P.
- **severity**: low. **kind**: port-only leak.
- **oracle**: `src/font/face/freetype.zig:100-105` (`errdefer hb_font.destroy()`,
  `errdefer lib.alloc.destroy(ft_mutex)`) covering `:120-126`; `src/font/face/coretext.zig:101`
  covering `:124`.
- **rustty**: `crates/rustty-font-shaper/src/face/freetype.rs`, `crates/rustty-font-shaper/src/face/coretext.rs`.

The reference arms an `errdefer` over the HarfBuzz font (and the boxed mutex on FreeType) before the
Debug-mode variation-axes dump, whose `try result.name(&buf)` and `try face.getMMVar()` can fail on a
variable font the face opened but whose name table or `fvar` does not read back. The port propagated
that error with `?` and dropped the half-built `Face`, which has no `Drop`, so both handles leaked. The
freetype comment claiming "no fallible op follows" was wrong. Both backends now free them on that arm,
in the reference's reverse-declaration order, leaving the FT face to the caller that owns it.

Proof on FreeType: `a_failed_variation_axes_step_destroys_the_harfbuzz_font` (a HarfBuzz destroy
callback) and, added by task R13-P2, `a_failed_variation_axes_step_frees_the_ft_mutex` (a test-only
global allocator that flags the free of the exact `ft_mutex` pointer). Negative control for the latter,
macOS 2026-09-22: with only the `ft_mutex` free removed, it fails ("the boxed ft_mutex is freed on the
error path, not leaked").

**state**: CLOSED by task R13-P (`b2ac07ec`).

> **2026-09-22 (task R13-D): the CoreText half no longer applies.** Merged commit `8d7e04d1` removed the
> CoreText face's HarfBuzz font (with its seam and test) on purpose: CoreText builds never shape with
> HarfBuzz (`Face::HARFBUZZ_SHAPER` is false there, enforced by a `const` assert), so `hb_font` is `()`
> and that error arm has nothing to free, as in the reference's macOS build, where
> `errdefer if (comptime harfbuzz_shaper) hb_font.destroy()` compiles away (confirmed by the R13-P2
> review). "Both backends now free them" is therefore true of FreeType only; the FreeType half above,
> re-proved by task R13-P2, stands.

## N54 — the snapshot decoder zero-filled an attacker-declared string length before reading it

- **id**: N54 (next free id checked 2026-09-11; N53 was the last). R13-G part A, A3.
- **found by**: the R13-G part A snapshot sweep; fixed by task R13-S.
- **severity**: low. **kind**: port-only resource defect (worse than the reference).
- **oracle**: `src/terminal/snapshot/terminal.zig:887-895` and `src/terminal/snapshot/hyperlink.zig:132-158`,
  where `alloc.alloc(u8, len)` reserves pages without touching them and `readSliceAll` copies only the
  bytes that exist before `EndOfStream`.
- **rustty**: `crates/rustty-vt/src/snapshot/terminal.rs` (`read_string`),
  `crates/rustty-vt/src/snapshot/hyperlink.rs` (`decode_alloc`).

Both sites did `try_reserve_exact(len)` and then `resize(len, 0)`, which WRITES every byte. A TERMINAL
record of a few hundred bytes declaring `pwd_len = 0xFFFFFFFF` therefore committed and zeroed 4 GiB
before the read failed, and the SCREEN cursor hyperlink repeated it per screen with its failure
swallowed. Measured on macOS 2026-09-11: such a record took **15.0 s** to be rejected, against
**0.01 s** once the length is bounded. The reference's own cost for the same bytes is virtual only,
which is why this is a port-only entry and the reference half is the latent resource defect UD66.

Fixed by the UD66 bound: the declared length is compared with the record's remaining payload before
anything is reserved or zeroed, so neither the reservation nor the fill happens. The tests assert this
positively, through the allocation tripwire, rather than relying on the error value — the unbounded
code returns the same error, just 4 GiB later.

The third site the sweep listed, `crates/rustty-vt/src/snapshot/continuation.rs` (`decode_alloc`), is
NOT an A3 site, but it is not bounded by the record either (corrected by task R13-S2; the first version
of this entry called it "already bounded in both trees", which overstated it). `continuation::decode`
compares the record header's `payload_len` with `max_bytes` before allocating (`continuation.zig:94`),
and `max_bytes` is the CALLER's configurable limit — the C decoder's `max_continuation_bytes` option,
65 MiB by default (`apc.Protocol.maxDefaultBytes()`, the Kitty default) and settable higher
(`crates/rustty-vt-c/src/snapshot.rs`, `DecoderOption::MaxContinuationBytes`). So a truncated
CONTINUATION record declaring up to that limit still makes the port reserve AND zero-fill it before the
read fails, where the reference only reserves.

The UD66 remaining-bytes bound cannot help there, so it was deliberately not applied: the length being
allocated IS the record's own declared payload, and `payload_remaining()` at that point returns exactly
`payload_len` (borrowed: the whole window, streaming: the whole limit), so the comparison is always
true. On the borrowed route the bytes are real, because the reader borrows only when the source already
buffers the whole payload; on the streaming route no bound on what the source still holds exists. The
residual port-only cost is therefore the zero-fill of at most the caller's own limit; it is bounded and
unchanged.

**state**: CLOSED by task R13-S.

## N55 — a child that exited before its kqueue watch was registered panicked the io thread

- **id**: N55 (next free id checked 2026-09-22; N54 was the last). Inventory M7.
- **found by**: the R13 inventory's code sweep (T1-5); trigger measured and fixed by task R13-C.
- **severity**: medium. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/termio/Exec.zig:296-305`.
- **rustty**: `crates/rustty-termio/src/exec.rs` (`processExit`).

The port's `r.unwrap()` mirrored `r catch unreachable`. XNU refuses an `EVFILT_PROC` registration for
a zombie with `ESRCH` (measured), and the registration is only submitted on the loop tick after
`threadEnter`, so a child that exits at once reached the panic. `NoSuchProcess` now takes
`processExitCommon` with libxev's no-status code 0. Reference defect: `UPSTREAM-DEFECTS.md` UD67.

**state**: CLOSED by task R13-C.

## N56 — the inherited working directory handed to the host leaked on every inherited surface

- **id**: N56. Inventory M15.
- **found by**: records F14; fixed by task R13-C.
- **severity**: low. **kind**: leak (reference defect, R13).
- **oracle**: `src/apprt/embedded.zig:1160-1178`.
- **rustty**: `crates/rustty-apprt/src/embedded.rs` (`newSurfaceOptions`, `InheritedWorkingDirectory`, `Surface::deinit`).

The port leaked a `CString` (`into_raw`) exactly as the reference leaks its `dupeZ`. The string is now
owned by the surface and freed on the next `newSurfaceOptions` or on `deinit`; the C ABI is unchanged.
Reference defect: `UPSTREAM-DEFECTS.md` UD68.

**state**: CLOSED by task R13-C.

## N57 — a failed keymap reload made `deinit` call `CFRelease(NULL)`, and a failed `init` leaked the source

- **id**: N57. Inventory M19.
- **found by**: records F13 (LIFETIMES L14) and the inventory consolidation; fixed by task R13-C.
- **severity**: low. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/input/KeymapDarwin.zig:60-119`.
- **rustty**: `crates/rustty-input/src/keymap_darwin.rs` (`reload`, `reinit`).

The port stored the `TISCopy…` result before checking it, so after a failed reload `deinit` released
NULL (a crash where the reference double-releases), and a layout-less source leaked on the init error
path. The copy is now committed only on success, released on the error path, and the old source is
released only after a successful reload. Reference defect: `UPSTREAM-DEFECTS.md` UD69.

**state**: CLOSED by task R13-C.

## N58 — the Metal display callback could still see a callback without its context

- **id**: N58. Inventory M91.
- **found by**: records F16; triaged FIX in `r13-verify-triage.md`; fixed by task R13-C.
- **severity**: low. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/renderer/metal/IOSurfaceLayer.zig:127-162`.
- **rustty**: `crates/rustty-renderer/src/metal/iosurface_layer.rs` (`set_display_callback`, `display`).

The port's atomics removed the formal data race but kept the reference's store order (callback, then
context), so a main-thread `display` between the render thread's two stores still called the callback
with a null renderer; its PORT NOTE wrongly said both sides ran on the main thread. The context is now
stored first when installing, `display` skips a null context, and the note is corrected. Reference
defect: `UPSTREAM-DEFECTS.md` UD70.

**state**: CLOSED by task R13-C.

## N59 — a `window-padding` of 2^31 or more panicked on the padding sums

- **id**: N59. Not in the inventory (found by task R13-P; decided by task R13-C).
- **found by**: task R13-P (the `scaledPadding` cast); task R13-C (the downstream sums).
- **severity**: low. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/Surface.zig:437-455`; `src/renderer/size.zig:72`, `:199-214`.
- **rustty**: `crates/rustty-render-types/src/lib.rs` (`sub_padding`, `blank_padding`, `balance_padding`);
  `crates/rustty-core/src/surface.rs` (`scaledPadding`, note only).

The port's saturating `as u32` in `scaledPadding` looked safe, but two paddings of 2^31 or more
(`window-padding-x = 2147483648`, or anything the cast saturated) overflowed `left + right` in
`sub_padding`, and release keeps overflow checks, so the port panicked where the reference does. The
padding sums now saturate, leaving a one-cell grid. Reference defect: `UPSTREAM-DEFECTS.md` UD71.

**state**: CLOSED by task R13-C.

## N60 — a mouse button mask of 192..=195, or an X10 byte below 32, panicked the vaxis parser

- **id**: N60. Inventory M23.
- **found by**: records F7; the X10 half by task R13-C; fixed by task R13-C.
- **severity**: low. **kind**: missing-branch (reference defect, R13; upstream libvaxis).
- **oracle**: libvaxis 0.6.0 `src/Parser.zig:688-746`.
- **rustty**: `crates/rustty-vaxis/src/mouse.rs` (`Button::from_u8`), `crates/rustty-vaxis/src/parser.rs` (`parse_mouse`).

The port's `Button::from_u8` hit `unreachable!()` for a masked value no button names, and the X10
branch's `- 32` underflowed for a raw byte below 32, both from tty input. Such events are now skipped,
consuming the whole sequence. Reference defect: `UPSTREAM-DEFECTS.md` UD72.

**state**: CLOSED by task R13-C.

## N61 — the vaxis SIGWINCH handler locked a mutex and ran callbacks in signal context

- **id**: N61. Inventory M24.
- **found by**: records F11; fixed by task R13-C.
- **severity**: low. **kind**: missing-branch (reference defect, R13; upstream libvaxis).
- **oracle**: libvaxis 0.6.0 `src/tty.zig:132-140`.
- **rustty**: `crates/rustty-vaxis/src/tty.rs` (`handle_winch`, `install_winch_handler`, `winch_dispatcher`).

The port kept upstream's handler body, and its module note called that "Zig's exact design". A
SIGWINCH on a thread holding the handlers mutex (`notify_winsize`, or the port's own
`deregister_winsize`) deadlocked it, measured by the negative control. The handler now writes to a
self-pipe and a dispatcher thread runs the callbacks. Reference defect: `UPSTREAM-DEFECTS.md` UD74.

**state**: CLOSED by task R13-C.

## N62 — the renderer thread's `stopCallback` trapped on an `Async` wait error

- **id**: N62. Not in the inventory (review of task R13-C, observation 7).
- **found by**: the review of task R13-C; fixed by task R13-C2.
- **severity**: low. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/renderer/Thread.zig:722-731`.
- **rustty**: `crates/rustty-core/src/renderer_seam.rs` (`stopCallback`).

The port kept the reference's `_ = r catch unreachable` as `unreachable!()`. On the epoll fallback
backend the stop eventfd's registration can fail (`ENOSPC` from the per-user epoll watch limit,
`ENOMEM`), and that error reaches the callback. It now re-arms, as `wakeupCallback` does for the same
error type. Reference defect: `UPSTREAM-DEFECTS.md` UD75.

**state**: CLOSED by task R13-C2.

## N63 — the IME point and the custom-shader cursor uniforms panicked on a saturated padding

- **id**: N63. Not in the inventory (review of task R13-C, observation 3).
- **found by**: the review of task R13-C; fixed by task R13-C2.
- **severity**: low. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/Surface.zig:2134`, `:2147`; `src/renderer/generic.zig:2351`, `:2356`.
- **rustty**: `crates/rustty-core/src/surface.rs` (`imePoint`); `crates/rustty-renderer/src/generic.rs`
  and `crates/rustty-renderer/src/generic/linux_impl.rs` (`update_custom_shader_uniforms_for_frame`).

After N59 the padding sums saturate, but the cursor-position sums did not. A saturated padding stored
by `contentScaleCallback` before the IO thread's resize, with the cursor off the first column or row,
overflowed them. They now saturate too. Reference defect: `UPSTREAM-DEFECTS.md` UD76.

**state**: CLOSED by task R13-C2.

## N66 — styled output on a terminal about 48,000 or more columns wide panics the port on the style refcount

- **id**: N66 (reserved range N66–N69 for task R13-S2; N61 was the last on `parity-closure` when written).
- **found by**: the R13-S review, while checking UD65's premise; measured again by task R13-S2.
- **severity**: medium. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/terminal/PageList.zig:535-540`, `src/terminal/Terminal.zig:1722`,
  `src/terminal/ref_counted_set.zig:365-379`.
- **rustty**: `crates/rustty-vt/src/pagelist/mod.rs` (`initial_capacity`), `crates/rustty-vt/src/terminal.rs`
  (`print_cell`), `crates/rustty-vt/src/ref_counted_set.rs` (`use_id`).

The port reproduced the reference's defect, from a different width: a terminal wide enough to leave the
standard page capacity gets 215 rows x cols cells, and the 65,535th cell printed in one style overflows
the `u16` style refcount (the cursor holds the other reference). Measured on macOS 2026-09-22 in the test
profile: `Terminal::init` at 48,151 x 3, `set_attribute(Bold)`, 70,000 `print('A')` panics "attempt to
add with overflow" at `ref_counted_set.rs:110`. `[profile.release]` keeps overflow checks on, so the
shipped build panics too (aborting the host through `extern "C"`); `release-fast` wraps. NOT a faithful
reproduction of the threshold: the port leaves the standard capacity at 48,036 columns shipped and 48,151
in a test build (macOS, from the port's `Capacity::adjust`), where Ghostty does so at 47,932 shipped
(macOS arm64; 46,908 on Linux aarch64) and 48,047 in a test build, because the port's style-set item is
smaller than the reference's. So from 47,932 to 48,035 columns (shipped, macOS) Ghostty aborts and the
port did not. Reference defect: `UPSTREAM-DEFECTS.md` UD80. Since N88 (task R13-L) the port's style set is
laid out with the reference's item geometry, and a sweep of the port's `Capacity::adjust` gives Ghostty's
thresholds exactly: 47,932 shipped and 48,047 in a test build on macOS, 46,908 shipped and 46,511 in a test
build on Linux aarch64.

**state**: CLOSED by task R13-W. `RefCountedSet` now saturates every increment at the maximum and never
decrements a count at the maximum, so the style is pinned and every cell keeps it; six tests in `terminal.rs`
(`ud80_wide_terminal_*`, at 50,000 columns, above every threshold measured in either tree) drive the VT
stream, print, the print batch, DECALN, reflow and a capacity-growth clone past the ceiling. See
`UPSTREAM-DEFECTS.md` UD80.

## N70 — `+crash-report` truncated the age of a very old crash file instead of handling it

- **id**: N70. Inventory P3 (with M30).
- **found by**: the R13 code sweep (T2); fixed by task R13-D.
- **severity**: low. **kind**: wrong-value (reference defect, R13).
- **oracle**: `src/cli/crash_report.zig:83-91`.
- **rustty**: `crates/rustty-cli/src/crash_report.rs` (`run_inner`).

The reference traps (`@intCast`) when a report's age exceeds 2^64 ns. The port converted with
`diff as u64`, which truncates an `i128`, so a crash file dated in the year 1400 (which Linux tmpfs and
btrfs store, measured) was listed as about 42 years old. The age now saturates to the largest
`Duration`. The inventory said the trigger was a future-dated file; a future mtime already takes the
reference's own "now" arm. Reference defect: `UPSTREAM-DEFECTS.md` UD99.

**state**: CLOSED by task R13-D.

## N71 — `scroll_viewport`'s `debug_assert!` aborted across `extern "C"` on an undeclared tag

- **id**: N71. Inventory P4 (with M37).
- **found by**: session log B6; fixed by task R13-D.
- **severity**: low. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/terminal/c/terminal.zig:1503-1514`.
- **rustty**: `crates/rustty-vt-c/src/terminal.rs` (`scroll_viewport`).

The port narrowed the C tag safely but kept a `debug_assert!(false, …)` that mirrored the reference's
"switch on corrupt value" trap. In debug and `release-safe` builds an embedder's undeclared tag
aborted the process inside the `extern "C"` export. The tag is C-caller input, so under R13 it is not
an invariant: every build now returns, and `scroll_viewport_invalid_tag` requires that. Reference
defect (a hardening proposal): `UPSTREAM-DEFECTS.md` UD113.

**state**: CLOSED by task R13-D.

## N72 — `Surface.init`'s error path freed the terminal while the renderer thread was running

- **id**: N72. Inventory M35 (the inventory called the port already safe).
- **found by**: task R13-D, verifying M35; fixed by task R13-D.
- **severity**: low. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/Surface.zig:582`, `:586`, `:725-737`.
- **rustty**: `crates/rustty-core/src/surface.rs` (`initErrdefers`).

The port's post-spawn unwind stopped and joined the renderer thread, which the reference does not, but
only after `self.io.deinit()` had freed the terminal the renderer reads, and it never stopped the io
thread. A failed io-thread spawn reached it. Both threads are now stopped and joined first, in
`deinit`'s order. Reference defect: `UPSTREAM-DEFECTS.md` UD104.

Correction (task R13-D2): task R13-D recorded this as "covered by reading only: `Surface::init` cannot
run in a unit test". That was wrong: six non-ignored tests in
`crates/rustty-apprt-c/tests/macos_window_host.rs` already run the full `rustty_surface_new` →
`Surface::init` with both threads spawned. What was missing was a way to fail the io-thread spawn. Task
R13-D2 added one (rustty-core's test-only `init-fault-inject` feature, enabled only from
rustty-apprt-c's dev-dependencies) and the test
`init_error_path_joins_both_threads_before_freeing_their_state`, which requires every spawned thread to
be stopped and joined before any of their state is freed, for a failed io spawn and for a failure after
it. Negative controls: the pre-R13-D order, and the io join alone removed, each fail it.

**state**: CLOSED by task R13-D (fix) and R13-D2 (test).

## N73 — the web-canvas `rustty_wc_face_new` export took the point size as `f32`, not `u16`

- **id**: N73. Not in the inventory (review of task R13-D, second new candidate).
- **found by**: task R13-D (concern 5) and its review; fixed by task R13-D2.
- **severity**: low. **kind**: signature divergence (R1).
- **oracle**: `src/font/face/web_canvas.zig:494` (`export fn face_new(ptr: [*]const u8, len: usize,
  pts: u16, p: u16) ?*Face`); only the inner `face_new_` (`:498`) takes `pts: f32`.
- **rustty**: `crates/rustty-font-shaper/src/face/web_canvas.rs` (`rustty_wc_face_new`, the
  `rustty_web_canvas.wasm` export; the `ghostty-wasm-abi` `face_new` already took `u16`).

The export took `pts: f32`, so a JS caller could pass a fractional size the reference cannot receive.
It now takes `u16` and widens it into `DesiredSize.points`, as the reference does. Consumers found: the
JS glue in `example/wasm-web-canvas/index.html` (`32.0`), `.port/tools/web-canvas-instantiate.mjs`
(four calls, `:69`, `:194`, `:242`, `:254`, `16.0`), and the symbol-retention table in `crates/rustty-web-canvas/src/lib.rs` (a
function-pointer cast, signature-agnostic). None depends on `f32`: each passes a whole number, which a
wasm `i32` parameter receives unchanged. `web-canvas-instantiate.mjs` passes after the change
(`ALL OK`, macOS, `wasm32-unknown-unknown` release). A `const` item pins the signature; negative
control: `pts: f32` restored fails to compile ("expected fn pointer `extern "C" fn(_, _, u16, _) -> _`",
wasm32 check). Not a reference defect: web_canvas is not built upstream at the pin (UD37, UD38).

**state**: CLOSED by task R13-D2.

## N74 — CoreText `Face::name` panicked on a font with no family name

- **id**: N74. Not in the inventory (review of task R13-D, first new candidate).
- **found by**: task R13-D (concern 4), measured by its review; fixed by task R13-D2.
- **severity**: low. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/font/face/coretext.zig:204-212`; `pkg/macos/text/font.zig:166-168`.
- **rustty**: `crates/rustty-font-shaper/src/face/coretext.rs` (`Face::name`).

`Face::name` used objc2's non-null `CTFont::family_name()`, which panics when `CTFontCopyFamilyName`
returns NULL, as it does for a font file with no `name` table (`Face::init` accepts such a font). The
lookup is now nullable and a missing name gives "unknown", `DeferredFace.familyName`'s answer for the
same NULL (task R13-C's S85-F2 fix made `DeferredFace` nullable the same way). Reference defect:
`UPSTREAM-DEFECTS.md` UD114.

**state**: CLOSED by task R13-D2.

## N75 — a failed `Surface::init` kept its font-grid reference, and the app's teardown then aborted

- **id**: N75. Not in the inventory.
- **found by**: task R13-D2, when the UD104 test freed its app after a failed `rustty_surface_new`.
- **severity**: low. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/Surface.zig:523` (`ref` with no `errdefer … deref`); `src/App.zig:141`.
- **rustty**: `crates/rustty-core/src/surface.rs` (`init`, `initErrdefers`); `crates/rustty-core/src/app.rs`
  (`deinit`).

Like the reference, the port released the grid reference only in `Surface::deinit`, so every
`Surface::init` failure after the `ref` leaked it. `App::deinit`'s `debug_assert!(font_grid_set.count()
== 0)` then fired at `rustty_app_free`, which (debug and `release-safe` builds) aborted across the C ABI:
"assertion failed: self.font_grid_set.count() == 0", then "panic in a function that cannot unwind".
Every error path after the `ref` now releases it, in the missing errdefer's LIFO slot. The assertion
stays: it states an invariant that now holds. Reference defect: `UPSTREAM-DEFECTS.md` UD115.

**state**: CLOSED by task R13-D2.

## N88 — the style set's item size made every page's layout, and so its rows, differ from the reference's

- **id**: N88 (reserved range N88–N90 for task R13-L).
- **found by**: the R13-S2 review ("new finding"), measured by the R13 page-layout investigation; fixed by
  task R13-L.
- **severity**: medium. **kind**: layout divergence (R1).
- **oracle**: `src/terminal/ref_counted_set.zig` (`Item`, `Layout.init`: `items_cap * @sizeOf(Item)` after
  `alignForward(table_end, @alignOf(Item))`), `src/terminal/style.zig:21-47` (`Style`, `Flags`,
  `Color`), `src/terminal/color.zig:540` (`RGB`), `src/terminal/page.zig` (`Page.layout`,
  `Capacity.availableBitsForGrid` / `adjust` / `maxCols`, `std_capacity`).
- **rustty**: `crates/rustty-vt/src/ref_counted_set.rs` (`RefItemGeometry`, `ItemStride`, `item_at`,
  `Layout::init`, `base_align`), `crates/rustty-vt/src/style.rs` and `crates/rustty-vt/src/hyperlink.rs`
  (the geometry impls), `crates/rustty-vt/src/page.rs` (`hyperlink_set_item_size`); on wasm32,
  `crates/rustty-vt/src/hyperlink.rs` `PageEntryId` (N91); the drift guard
  `.port/tools/gates/check_ref_item_geometry.py` (G-REFGEOM) with
  `.port/tools/gates/fixtures/ref_item_geometry_{probe.zig,golden.txt}` and
  `crates/rustty-vt/src/ref_geometry_golden_tests.rs` (task R13-L2).

The port's `Style` is 21 bytes at align 1 (`Color` 4/1, `color.RGB` 3/1, `Flags` 9/1) where the reference's
is 28 at align 4 (`Color` 8/4, `RGB` 4/4, `Flags` 2/2), so its style-set `Item` was 28/2 against 36/4. The
item array is laid out as `items_cap * size_of::<Item>()`, so the styles section of a standard page was
3,168 bytes against 4,000. Every other section is byte-identical, but the styles section feeds
`availableBitsForGrid`, so the rows of every page, `maxCols` and the standard size differed. MEASURED
(Zig 0.16.0 probe of `20abdb50a` on macOS arm64 and cross-compiled for Linux aarch64; the port before the
fix from the investigation's probe):

| | macOS shipped | macOS test | Linux shipped | Linux test |
|---|---|---|---|---|
| rows at 80 columns, reference / port before | 591 / 593 | 593 / 594 | 579 / 574 | 574 / 575 |
| `maxCols`, reference / port before | 47,931 / 48,035 | 48,046 / 48,150 | 46,907 / 46,499 | 46,510 / 46,614 |
| standard page bytes, reference / port before | 409,600 / 409,600 | 393,216 / 393,216 | 401,408 / 397,312 | 380,928 / 380,928 |

Every width from 1 to 119 differed; on macOS the port held more rows per page, on Linux shipped fewer.
Pages with 2,048 or more styles were 3–17% smaller than the reference's. Observable: the lines kept under
`scrollback-limit` (about +244 on macOS and −46 on Linux at the 50 MB default, 80 columns), page split
points and `scrollback-limit-lines` pruning, snapshot HISTORY page boundaries, grid refs `(node, y)`, the
inspector's page figures, the Linux pool item size, and the wide-terminal thresholds of UD64, UD65, UD80
and N66. Two records called the difference "not observable": the module doc of
`crates/rustty-vt/tests/std_size_golden.rs` and `.port/phase-2/ALLOC_BENCH_V2.md` §2 (both corrected).
The `Style.Color` PORT NOTE also claimed its 4 bytes were "exactly what the reference specifies for a
color", confusing `PackedStyle`'s 24-bit hash key with the stored 8-byte union (corrected).

**fix**: the port keeps its compact types (the `Style` compare, hash and clone paths behind the published
perf lead rely on them) and lays the set out with the REFERENCE's item geometry. `RefItemGeometry` gives
each value type the reference's `@sizeOf(Item)` / `@alignOf(Item)`, derived from `#[repr(C)]` shadows of
the reference's declarations (`style.rs` `RefShadowItem`: 36/4; `hyperlink.rs`: 48/8, the same as the
port's). `ItemStride` checks at compile time that the port's item fits its slot and stays aligned.
`Layout::init`, `base_align` and `init`'s zeroing use it, and every item address is `items + id * 36`
through `item_at`: `add`, `add_with_id`, `add_with_id_is_pure_ref_bump`, `use_id`, `use_multiple`, `get`,
`get_at`, `release`, `release_multiple`, `ref_count`, `delete_item`, `lookup`, `insert`, the iterator, the
page integrity walk (`page_impl.rs`, which viewed the items as a `&[Item]`), and the inspector's style and
hyperlink lists (`rustty-inspector` `widgets/page.rs`, likewise). Each item sits where the reference puts
it, with the port's 28 bytes at the start of its 36-byte slot and the tail zero. `hyperlink_set_item_size`
is the stride. The snapshot wire format is unchanged.

**tests**: `ref_item_geometry_matches_the_reference_probe` (`style.rs`: the shadows, both strides and
`Layout.init(n)` for n = 0, 1, 2, 16, 128 against the probe); `set_items_are_addressed_at_the_reference_stride`
(`style.rs`: a set filled to capacity, read back through every access path and at the raw offset
`items_start + id * 36`, slot tails zero, then a third released and refilled through `add_with_id`);
`page_geometry_matches_the_reference` (`page_tests.rs`: rows per page at 13 widths from 1 to 10,000,
`maxCols`, `availableBitsForGrid`, the standard size and four non-standard sizes, test and shipped budgets,
on macOS and on Linux aarch64); `std_size_golden` (styles 4,000, a Linux 401,408 pin, shipped rows per page
and `maxCols`). Negative controls (2026-09-22, each mutation applied by a script that refuses a pattern
matching other than once, file restored after each): addressing one path with `items.add(id)` instead of
`item_at` — `get`, `get_at`, the iterator, `use_id`, `ref_count`, `insert`'s placement, `lookup`, `init`'s
metadata loop, or `item_at` itself — turns the addressing test red; laying the set out with
`size_of::<Item>()` turns the geometry and page tests red; with `align_of::<Item>()` the `Layout.init(1)`
check goes red. Drift guard (task R13-L2): G-REFGEOM re-measures the reference's item geometry (the style
and hyperlink `Set.Item`, `Style`, `Style.Color`, `color.RGB`, `Style.Flags`, `PageEntry`, `PageEntry.Id`,
`Offset(u8).Slice`, the item metadata) with a checked-in Zig probe against the pinned oracle, for
aarch64-macos, aarch64-linux-gnu and wasm32-wasi, and compares it with a checked-in golden;
`ref_geometry_golden_tests` checks the port's shadows, both strides and its own types against the same
golden (64-bit lines on the host, wasm32 lines through a layout model checked against the host). Negative
controls: a golden line edited (wasm32 hyperlink item 28 -> 32) turns the gate red on exactly that line
and the test red ("the port's reference shadow", 32 against 28); an aarch64-macos style item edited 36 ->
40 turns the test red.

**wasm32** (task R13-L2, N91): on a 4-byte `usize` the reference's hyperlink `Set.Item` is 28/4
(`PageEntry.Id` 12/4, `PageEntry` 20/4, `Offset(u8).Slice` 8/4) and its style item is 36/4, as on
64-bit. The port's hyperlink item is 28/4 there too since `PageEntryId` shares its payload bytes (N91),
so the wasm32 page layout uses the reference's geometry for both sets; `ItemStride`'s fit assert is what
caught the 32-byte item, and G-XTARGET (`check_cross_targets.sh`) builds the target.

**performance** (macOS arm64, `rustty-bench` release-fast, r13-w base against the fix, alternating runs):
`+terminal-stream` medians within ±1.3% on ascii, two SGR-heavy corpora and a truecolor corpus with
thousands of distinct styles; `+screen-clone` render ns/op (loops 1 and 5; render modes honour `--loops`)
+1.1% on SGR, −2.6% on truecolor. The clone figures are WALL TIME, not ns/op: `--loops` does not apply to
`--mode=clone` (it always runs 1,000 clones), so a loops sweep there is meaningless; the R13-L review
measured clone as clone minus noop wall time (N = 21, rotated), which isolates the 1,000 clones. On the
truecolor corpus the fix is +5.1% against base (the author's plain wall time: +6.4%); a build with the
reference layout but the old 28-byte addressing is at parity with the fix (0.991), and the fix's code at
the old geometry is at parity with base (1.005): the cost is the reference's larger page for a page with
thousands of styles, not the stride. SGR mix and ascii are unchanged (±1%). Against Ghostty at the pin
(`ghostty-bench` ReleaseFast, same method): on the standard ws10 clone corpus (`ascii-small.bin`) the
port is 2.70x faster (15.7 against 42.3 ms per 1,000 clones), but on style-heavy screens Ghostty is faster
at clone — about 6% on truecolor (93.9 against 100.3 ms) and about 16% on an SGR mix (55.5 against 66.3) —
a gap that PREDATES this task (at `r13-w` Ghostty was already about 2% and 16% faster); N88 widened the
truecolor gap by its +5%. Recorded as N93 (open). Task R13-L2's `PageEntryId` and `delete_item` changes
leave the native sizes at 48/8 and cost nothing: clone minus noop, N = 21, rotated, after against before
0.998 / 1.004 / 0.999 (truecolor / SGR / ascii).

**state**: CLOSED by task R13-L.

Update (P7-W1 T3, 2026-09-28): upstream `d2ff6d77a` (at the new pin `12752b2ac`) aligns the cell array to
`std.atomic.cache_line`, lays the metadata out as one `MetaLayout` block after the grid, and rewrites
`maxCols`/`adjust` in bytes (`availableBitsForGrid` became `availableBytesForGrid`); `6112935a2` shrank the
hash-map header (grapheme map 10,768 -> 10,760, hyperlink map 912 -> 900). Both geometry goldens were
re-probed at `12752b2ac` with the checked-in Zig probe `.port/phase-7/probes/p7w1t3_page_geometry_probe.zig`
on macOS arm64 and on Linux aarch64 (`rvm`), at 512 and 8,192 grapheme bytes, and the port's ported layout
matches every value: cells start at 1,792 (was 1,720); the standard sizes are unchanged; `maxCols` fell by
13 everywhere (macOS 47,931 -> 47,918 shipped, 48,046 -> 48,033 test; Linux 46,907 -> 46,894 shipped,
46,510 -> 46,497 test); rows per page moved at a few widths (macOS: 1 column 24,023 -> 24,017 test, 40
columns 1,169 -> 1,168 shipped, 1,000 columns 47 -> 48 test; Linux shipped: 80 columns 579 -> 578, 40
columns 1,144 -> 1,143; Linux test: 1 column 23,255 -> 23,249). `page_geometry_matches_the_reference`
renamed its field to `available_bytes`; `std_size_golden` pins `cells_start` too. The port's
`std.atomic.cache_line` is `rustty_zigstd::atomic::CACHE_LINE`, the reference's per-arch table.

## N89 — the `hyperlink.Set`, `Page` and `PageList` node headers are larger than the reference's

- **id**: N89.
- **found by**: the R13 page-layout investigation.
- **severity**: low. **kind**: representation (heap objects only).
- **oracle**: `src/terminal/hyperlink.zig:211` (`Set`'s context: `page: ?*Page`, `src_page: ?*const Page`).
- **rustty**: `crates/rustty-vt/src/hyperlink.rs` (`SetContext`).

The hyperlink set's context stores its page pointers as exposed-address `Option<usize>` (Stacked
Borrows), 16 bytes each with no null niche, where the reference's `?*Page` is 8. So the `hyperlink.Set`
header is 168 bytes against 152, `Page` 416 against 400, and a `PageList` node 480 against 472. These live
in heap objects, not in page memory: no page section, row count or snapshot byte changes. They show only
through allocator-callback sizes (`NodePool` items). Deliberate; recorded, not fixed.

**state**: RECORDED (deliberate).

## N90 — the UD64 page-size boundary test fails on Linux aarch64

- **id**: N90.
- **found by**: task R13-L, running the `rustty-vt` suite on `rvm`.
- **severity**: low. **kind**: test portability.
- **rustty**: `crates/rustty-vt/src/snapshot/page.rs`
  (`decode_rejects_a_capacity_whose_exact_layout_is_one_past_the_limit`, task R13-W).

The test hard-codes a grapheme budget, 1,382,748,161, measured on macOS, at which a 48,151-column page
lays out at exactly 2^32 bytes. With a 4 KiB `page_size_min` the same capacity lays out at 4,294,955,008
bytes (12,288 short of 2^32), so the first assertion fails on Linux aarch64. It fails on the `r13-w` base
too (measured 2026-09-22 at `551e7a30` on `rvm`), so N88 did not cause it. The decoder's check itself is
correct; the test needs its boundary budget found per target (the smallest budget whose layout exceeds
`MAX_PAGE_SIZE`) instead of a constant.

**state**: **CLOSED** (triage T-1, 2026-09-28): confirmed; the R13-L3 update below is the evidence (Linux aarch64 run on `rvm`, 3,089 passed, boundary found at 1,382,759,425 = the reference's Linux value, mutation RED). The "state stays OPEN" sentence in the R13-L2 update is historical. Was: CLOSED by task R13-L3, on the R13-L2 review's Linux run (see the second update).

Update (task R13-L2): task R13-W2 replaced the constant with a binary search over the platform's own
`Page::layout` that asserts it reaches exactly 2^32, so the test no longer depends on the page size; its
remaining macOS arm64 pin is now the reference's 1,382,747,137 (a Zig probe of `Page.layout` at the pin;
1,382,748,161 was the port's value before N88). Not yet re-run on Linux aarch64, so the state stays OPEN
until a VM run confirms it.

Update (task R13-L3): confirmed on Linux aarch64. The R13-L2 review ran the port on `rvm` at `r13-l2`
`0e805945` (which `r13-l3`'s `2e5b52ce` cherry-picks onto `parity-closure`; review-R13L2 §2):
- `cargo test -p rustty-vt --features oniguruma --lib` passes, 3,089 passed and 0 failed, the boundary
  test included;
- the test's search finds 1,382,759,425, with a layout of exactly 2^32 and `PAGE_SIZE_MIN` 4,096. That
  is the reference's Linux value: a Zig probe of `Page.layout` at the pin, cross-built for
  aarch64-linux and run on `rvm`, gives 1,382,759,425 (2^32) against 1,382,759,424 (2^32 − 4,096);
- the mutation `> MAX_PAGE_SIZE as u64` → `> MAX_PAGE_SIZE as u64 + 1` in `check_native_page_size`
  turns the test RED on Linux (`snapshot/page.rs:2137`: `Err(InvalidDimensions)` against `Ok(Capacity {
  … grapheme_bytes: 1382759425 … })`).

## N91 — on wasm32 the port's hyperlink set item was 32 bytes against the reference's 28

- **id**: N91 (range N91–N93, task R13-L2).
- **found by**: the R13-L review (blocking item 1: `cargo build -p rustty-vt-c --target
  wasm32-unknown-unknown` failed on `ItemStride`'s fit assert once N88 laid the set out at the reference's
  stride); fixed by task R13-L2.
- **severity**: medium. **kind**: layout divergence (R1), wasm32 only.
- **oracle**: `src/terminal/hyperlink.zig:84` (`PageEntry.Id = union(enum) { explicit: Offset(u8).Slice,
  implicit: size.OffsetInt }`), `:80` (`PageEntry`), `:211` (`Set`); `src/terminal/size.zig`
  (`Offset(T).Slice`).
- **rustty**: `crates/rustty-vt/src/hyperlink.rs` (`PageEntryId`).

The port's `PageEntryId` was a byte-complete `#[repr(C)]` struct (P3 F1) holding BOTH payloads side by
side plus the tag: 16 + 4 + 1 + 3 = 24 on 64-bit, equal to the union's 24 only by coincidence, and
8 + 4 + 1 + 3 = 16 on wasm32 against the union's 12. MEASURED (Zig 0.16.0 probe of `20abdb50a` for
`wasm32-wasi`, by the review and again by G-REFGEOM): `PageEntry.Id` 12/4, `PageEntry` 20/4, `Set.Item`
28/4; the port's were 16/4, 24/4 and 32/4. So on wasm32, before N88, the hyperlink section was laid out
at 32 bytes per item (`hyperlink_bytes_default` 4 x 32 = 128 against 112), and after N88 the port's
32-byte item did not fit the reference's 28-byte slot, which the compile-time assert refused.

**fix**: `PageEntryId` holds ONE `OffsetSlice` payload, a raw `u8` tag and named tail padding
(`[u8; size_of::<usize>() - 1]`): `.explicit` is the slice, `.implicit` is its `u32` offset field — the
union's byte 0, where the reference puts the `u32` too — with `len` zero. No `union` and no `unsafe`:
both payloads are plain `u32`s at the same place, every read stays behind the tag-checked accessors, and
every byte stays initialized (the P3 F1 requirement). Sizes: 24/8 on 64-bit (unchanged), 12/4 on wasm32;
`PageEntry` 40/8 and 20/4; the hyperlink item 48/8 and 28/4, the reference's on both. Users checked: the
constructors and accessors are the only field access (the fields are private); `dupe`, `hash`, `eql` and
`free` (tag, then the payload through the accessor); snapshot encode (`snapshot/page.rs`, through
`HyperlinkId`) and decode (`snapshot/hyperlink.rs`, through the constructors); page clone (a whole-memory
copy, or `dupe` through `add_with_id_ptr`); `page_impl.rs`, `screen.rs`, `pagelist/resize.rs`; the
inspector (`widgets/page.rs`, `widgets/pagelist.rs`, accessors only). The C ABI has no `PageEntry`.

**tests**: `const` asserts beside the type (size `3 * size_of::<usize>()`, align, and the tag directly
after the payload, which the side-by-side shape fails on 64-bit too); `ItemStride` on wasm32;
`ref_geometry_golden_tests` (`the_port_matches_the_reference_geometry_golden`,
`the_wasm32_hyperlink_item_is_the_references`, `the_layout_model_reproduces_the_host`) against the
G-REFGEOM golden. wasm32 tests cannot run on the host, so the wasm32 geometry is pinned by a layout model
computed from constants (the fields' size/align pairs, `repr(C)` in order, default repr sorted by
alignment) that is itself checked against the host's real layouts at `usize` = 8. Negative control: the
side-by-side shape restored turns the host build red (the tag-offset assert) and the wasm32 build red
(the size assert, the hyperlink item assert and `ItemStride::SIZE`). `check_cross_targets.sh`: GREEN,
wasm32-unknown-unknown and wasm32-wasip1 built (the Windows, FreeBSD and Android legs skip on this host
for want of a toolchain, as before); `rustty-wasm` and `rustty-web-canvas` build for wasm32.

The model alone did not pin the REFERENCE side on wasm32: it retypes the shadows' fields by hand, so a
shadow field changed to a wrong 32-bit type passed it (the R13-L2 review: `RefShadowSlice.len` as `u64`
kept both the host tests and the wasm32 build green, with the wasm32 hyperlink item silently 32/8). Task
R13-L3 added `#[cfg(target_pointer_width = "32")]` `const` asserts pinning the shadows to the golden's
`wasm32-wasi` lines: `RefShadowSlice` 8/4, `RefShadowPageEntryId` 12/4, the hyperlink `RefShadowItem`
and `REF_ITEM_SIZE/ALIGN` 28/4 (`hyperlink.rs`), and `RefShadowColor` 8/4, `RefShadowStyle` 28/4, the
style `RefShadowItem` and `REF_ITEM_SIZE/ALIGN` 36/4 (`style.rs`). Negative control: `len: u64` makes
`cargo build -p rustty-vt-c --target wasm32-unknown-unknown --release` fail (E0080 on the slice
assert). G-REFGEOM itself now follows the oracle-gate rules and runs in ci.sh (see `.port/tools/CI.md`).

**state**: CLOSED by task R13-L2.

## N92 — `delete_item` reset a dead set item with a whole-`Item` typed write

- **id**: N92.
- **found by**: the R13-L review (§1, non-blocking; pre-existing at `r13-w`); fixed by task R13-L2.
- **severity**: low. **kind**: memory initialization (Rust-side), no reference counterpart.
- **rustty**: `crates/rustty-vt/src/ref_counted_set.rs` (`delete_item`).

`delete_item` wrote `*item_at(items, id) = Item::default()`. A typed write of the whole item stores the
default value's unspecified padding over the item's padding byte (the port's `Style` is 21/1 and the
metadata 6/2, so each 28-byte style item has one) — de-initializing it inside the page mapping that
`CompressedPage::init` and every whole-page copy read as raw bytes. `insert`'s reap and final placement
were already field-wise for exactly this reason (its PORT NOTE and `init`'s). `delete_item` now writes
`value` and `meta` separately, so the item's other bytes keep their initialized value.

**tests**: `delete_item_leaves_the_item_padding_byte_alone` (`style.rs`): item 2's padding bytes are set to
a sentinel, `add`'s trim loop deletes the dead item 2, and the sentinel must survive. Without Miri an
uninitialized byte cannot be observed, so the test asserts the stronger "untouched". Negative control: the
typed write restored turns it red in a debug build ("item 2 padding byte 27").

**state**: CLOSED by task R13-L2.

## N93 — on style-heavy screens Ghostty clones faster than the port

- **id**: N93.
- **found by**: the R13-L review (§5), measuring task R13-L's clone cost.
- **severity**: low. **kind**: performance.
- **oracle**: `src/terminal/page.zig` (`Page.clone`, `cloneFrom`), `src/terminal/ref_counted_set.zig`.
- **rustty**: `crates/rustty-vt/src/page_impl.rs` (page clone), `crates/rustty-vt/src/ref_counted_set.rs`,
  `crates/rustty-bench` (`+screen-clone`).

MEASURED by the R13-L review (macOS arm64; `rustty-bench` release-fast against `ghostty-bench` ReleaseFast
built from `20abdb50a`; `+screen-clone --mode=clone` minus `--mode=noop` wall time, which isolates the
1,000 clones because `--loops` does not apply to `--mode=clone`; N = 21 runs, binaries rotated; medians,
ms per 1,000 clones):

| corpus | port `r13-w` | port `r13-l` | Ghostty `20abdb50a` |
|---|---:|---:|---:|
| truecolor, thousands of distinct styles (`styles-small.bin`) | 95.4 | 100.3 | 93.9 |
| SGR mix (`sgr-small.bin`) | 66.0 | 66.3 | 55.5 |
| `ascii-small.bin` (the ws10 `screen-clone.clone/small` corpus) | 15.8 | 15.7 | 42.3 |

On the standard corpus the port is 2.70x faster (2.78x against the historical oracle `4749c4e93`), but on
style-heavy screens Ghostty is faster: about 6% on truecolor and about 16% on the SGR mix. The SGR gap
predates N88 (it is present at `r13-w`); on truecolor Ghostty was about 2% faster at `r13-w`, and N88's
reference-sized style section added about 5%. The cause is not investigated yet (candidates: the port's
per-style clone path through `add_with_id_ptr` / `use_multiple`, and its hashing). Re-measured by task
R13-L2 with the same method, after against before: 0.998 / 1.004 / 0.999, no change.

**cause** (task S-4, MEASURED): not the hashing. A fresh page numbers styles in first-seen order, so on a
style-heavy screen most style runs land on a different id in the clone than in the source. The port's
run batching (`clone_partial_row_from_inner`) only recognised the run whose style keeps its id
(`add_with_id_is_pure_ref_bump`); every other run fell back to one full `add_with_id` per cell, which
takes `add_context` (a hash and a probe). An xctrace profile of 12,000 clones of `sgr-small.bin` put 52% of
samples under the per-cell fallback (`lookup_context` 29% self, `add_context` 19% self). Ghostty pays the
same per-cell lookups (it has no batching), which is why the two were close; on `ascii-small.bin` (one
style, id preserved) the port's batching already made it 2.7x faster.
**fix** (commit `5805b4dc`): `add_with_id_pure_ref_bump_target` mirrors `add_with_id`'s branch selection
and returns the id a repeated call would purely bump in every arm (same id, dead id found elsewhere,
`add_context` with no trim and the value present); the run-mates get one `use_multiple` on that id and a
copy with the style id replaced. The style context's `deleted` is a no-op and the bump saturates (UD80),
so the end state is identical. Test `page_cloneFrom_style_runs_match_per_cell_reference`: the cloned page
is byte-identical to the reference's per-cell body (and `next_id`, `living`, `max_psl`, `psl_stats`
equal) for fresh, live-conflict and dead-id destinations, with positive evidence (a test-only counter)
that the remap batch ran; controls: mates keep the source id (integrity `MismatchedStyleRef`), bump the
source id (assertion), the old predicate ("remapped runs batched: 0").
**measured** (task S-4; oracle = Ghostty's bench built at the pin, `.port/tools/perf/README.md`; corpora
regenerated: `sgr-small.bin` = `ghostty-gen +styled --seed=42 --style-rate=0.8`, `styles-small.bin` =
random 24-bit foreground per 4-16-character run, Python `random.Random(93)`, both 262,144 B; the
originals were not kept): instructions retired, clone minus noop, per 1,000 clones: SGR mix 1,581M before,
550M after, Ghostty 1,321M; truecolor 1,752M before, 642M after, Ghostty 1,484M (instruction counts are
deterministic and carry the closure). PROVISIONAL (load 21-25, above `IDLE_MAX` 2.0): CPU time (user+sys,
interleaved rounds, N=21), clone minus noop: SGR mix 72.7 ms before, 28.5 ms after, Ghostty
66.4 ms (0.43x); truecolor 83.3 ms before, 38.5 ms after, Ghostty 79.7 ms (0.48x). `screen-clone.clone/small`
unchanged (instructions 1.001x, cycles 0.997x over 41 rounds). The authoritative idle ws10 run is still
owed (task S-4 report).

**state**: CLOSED (task S-4).

## N80 — the row-copy retry loops degrade unstorable cells to defaults instead of crashing at the style-set ceiling

- **id**: N80. Not in the inventory.
- **found by**: task R13-X, when a reference probe confirmed the crash the R13-W2 review flagged.
- **severity**: low–medium. **kind**: missing-branch (reference defect, R13).
- **oracle**: `src/terminal/Screen.zig:747` (`clonePartialRowGrowCapacity`) and
  `src/terminal/PageList.zig:5108` (`cloneRowGrowCapacity`), both `@panic("increaseCapacity OutOfSpace")`.
- **rustty**: `crates/rustty-vt/src/screen.rs` (`clone_partial_row_grow_capacity`),
  `crates/rustty-vt/src/pagelist/grow.rs` (`clone_row_grow_capacity`),
  `crates/rustty-vt/src/page_impl.rs` (`clone_partial_row_degrade`).

The reference's row-copy retry loops `@panic` when `increaseCapacity(.styles)` returns `OutOfSpace`: the
callers (IL/DL, a scroll, an erase across a page boundary) have already rotated rows, so a crash is
chosen over a half-mutated page list. This is reachable on an ORDINARY 80×24 terminal from about 1.2 KB of
output (task R13-X3): 32 foreground colours whose unseeded, deterministic style hashes share their low 16
bits stay clustered at every capacity doubling and drive the ref-counted set's PSL-overflow OutOfMemory
arm (`ref_counted_set.zig:268`), so `increaseCapacity(.styles)` climbs to the 65,535 ceiling and returns
`OutOfSpace` on a page holding only ~32 live styles — no wide terminal and no full set needed. (A SECOND
route needs a wide terminal, >= 47,932 columns on shipped macOS, whose page's style set is genuinely full
of 53,247 distinct live styles at the ceiling; that width threshold belongs to that route only.) MEASURED
reachable in both ReleaseSafe (SIGABRT) and ReleaseFast (`panic:` trap) against a scratch `libghostty-vt`
build: the 80-column PSL route (task R13-X3) and the wide distinct-styles route (task R13-X). Every width
is reachable via the PSL route; task R13-X's "a terminal narrower than 9,585 columns cannot reach it" was
wrong (it missed the PSL arm) and is removed. The port instead completes the copy: when a capacity increase returns
`OutOfSpace` it degrades THAT dimension only and retries, so the other dimensions (graphemes, hyperlinks,
strings) still grow, a style already in the set is still stored, and only the parts of cells that fail
in a degraded dimension fall back to the default — at the style ceiling, the style of cells that bring a
new style; codepoints and row flags are kept. Logged once per copy. Each dimension degrades at most
once, which bounds the loop (UD131 "port handling"). This mirrors the reference's own
degrade-to-default precedent (`increaseCapacity` / `manualStyleUpdate`, UD65). Reference defect:
`UPSTREAM-DEFECTS.md` UD131.

Update (task R13-X2): task R13-X's first version degraded every dimension as soon as the style set
returned `OutOfSpace`, dropping graphemes and hyperlinks that could still grow (the R13-X review: 2,968
of 3,000 graphemes lost, grapheme capacity left at 512 bytes); the per-dimension degrade above replaced
it, with mixed-row tests against a control copy, release-only VT tests of every route, a once-per-copy
log test and a debug re-clone counter (UD131 "test").

Update (task R13-X3): the R13-X2 review found the reachability was understated — the crash is reachable
on an ORDINARY 80×24 terminal via the style set's PSL-overflow arm, not only on a wide terminal with a
full live set. UD131's severity is raised to HIGH (a remotely reachable DoS: about 1.2 KB of ordinary
output crash Ghostty at 80×24) and its `upstream status` says to report it privately via a GitHub
security advisory. An 80-column debug test (`ud131_delete_lines_degrades_on_an_ordinary_80_column_terminal`,
`terminal.rs`) and a control (31 colours) run in the default suite and on Linux aarch64, with a
restore-the-`@panic` negative control; a pure table test covers the non-style degrade arms; the
release VT tests now assert integrity through `verify_integrity` (real in release); the log wording is
"cell part(s)". The port behaviour is unchanged — it already degrades this route correctly.

**state**: CLOSED by task R13-X (per-dimension degrade: task R13-X2; reachability and records corrected
by task R13-X3).

## N81 — the log-reconcile gate cannot declare a PORT-ONLY emission, so each one masks one future reference-site loss

- **id**: N81. Not in the inventory.
- **found by**: task R13-X3, closing the R13-X2 review's blocking item 2 (the G-LOGFILE allowance that hid a real missing reference site).
- **severity**: low (a gate-sensitivity gap, not a port defect). **kind**: gate-gap.
- **gate**: `.port/tools/gates/check_log_reconcile.py` (G-LOGFILE), reconciling against `.port/parity-closure/LOG-PORTMAP.tsv`.

The gate counts each port arm's live emissions and compares against the reference file's live `log.*`
sites, with `deficit = max(0, reference − port)` and an allowance ratchet. It has two declaration files:
`LOG-TEXT-DIVERGENCES.tsv` (a reference site whose message text the port changed) and
`LOG-DEAD-SITES.tsv` (an unreachable reference site subtracted from the expectation). NEITHER can express
a PORT-ONLY emission — a diagnostic the port emits that has no reference counterpart. Such an emission is
silently counted toward its file's port total, and because a surplus is clamped (`max(0, …)`), it cannot
even be seen: it OVER-CREDITS the file by one and thereby masks one future loss of a real reference site
in that file (the gate stays green until a SECOND site is also lost).

Concretely (task R13-X3): `src/terminal/PageList.zig` has 23 live reference sites. The port arm
(`pagelist/{erase,grow,mod,resize}.rs`) emits 24 — the 23 reference-matched sites plus one PORT-ONLY R13
diagnostic, the row-copy degrade warning `log_row_degrade` (call site `grow.rs:594`, UPSTREAM-DEFECTS.md
UD131), which the reference does not have (it `@panic`s there instead). MEASURED: with `PageList.zig:775`
(`assertIntegrity`'s `log.err`) restored as `mod.rs:1943`, the arm reads 23 reference / 24 port, deficit
0, GREEN. Removing `:775` again leaves 23/23 — still GREEN — even though a real reference site is now
missing: the +1 over-credit absorbs it. Only once the over-credit is subtracted (MEASURED by stubbing
`log_row_degrade` so the gate stops counting its call site) does removing `:775` drop the arm to 22/23,
deficit 1, RED. So the +1 currently masks exactly one future reference-site loss in `PageList.zig`.

Tasks R13-X/R13-X2 hit this: R13-X added the port-only line (raising 22/23 to 23/23) and R13-X2 tightened
the allowance from 1 to 0 citing the wrong precedent (R13-P, which had added a genuine counterpart of a
reference site, not a port-only line). The correct precedent is `src/apprt/embedded.zig` (commit
`d130c504`), where port-only D11 range guards were declared a deliberate over-credit and the ratchet
forced allowance 0. The LOG-PORTMAP reason is rewritten to that form by task R13-X3.

**proposed remedy** (NOT implemented here): a message-keyed `LOG-PORT-ONLY.tsv` whose rows are SUBTRACTED
from the port count of a declared reference file. It would carry a `#!port-only-cap` two-way ratchet (a
missing directive is BLOCKED), a self-test arm, and a RED check if a declared message is absent from the
port. With that in place, the `PageList.zig` +1 (and the `embedded.zig` over-credit, which should then be
revisited) would be declared and subtracted, so removing `:775` would correctly go RED. This is a gate
change to `check_log_reconcile.py` and its ledger; it is recorded, not applied, in this round.

Update (task tail-s1): the remedy is implemented. `.port/parity-closure/LOG-PORT-ONLY.tsv` declares
each port-only emission by oracle file, port file, innermost enclosing Rust `fn` (optionally `fn#k`,
the k-th match, for textually identical `#[cfg]` twins) and a substring of the emission's call text,
with a kind from a closed set (`r13-handling`, `rust-only-path`, `abi-guard`, `instrumentation`,
`expansion`, `open-finding`) and a mandatory reason. G-LOGFILE subtracts each from every portmap arm
that contains its port file before comparing. A row that selects no live emission is RED (stale), one
that selects several is RED (ambiguous), two rows selecting one emission are RED, a port file outside
the named reference file's arms is RED, and the row count sits on a two-way `#!port-only-cap` ratchet;
a missing directive or a malformed row is BLOCKED (exit 3). `expansion` covers the second shape the
same gap hid: extra static copies of ONE reference site. Self-test arms cover the subtraction, the
masked-loss case in both states, stale, ambiguous, `fn#k`, duplicate, both cap directions, kind,
reason, arm membership and five loader cases; mutations removing the subtraction, the ambiguity check
and the duplicate check each fail their arm. MEASURED on the real tree: with the `PageList.zig` row
declared the arm reads 23/23, allowance 0, GREEN; deleting the `PageList.zig:775` port
(`pagelist/mod.rs` `assert_integrity`'s `log::error!`) now turns the gate RED (`reference=23 port=22
deficit=1 allowance=0`), restored afterwards. The `embedded.zig` over-credit (`d130c504`) was the 3
port-only D11 range guards in `rustty-apprt-c/src/lib.rs` (`rustty_app_key`, `rustty_surface_key`,
`rustty_inspector_key`); they are declared, and the row now reads 52/54 with allowance 2 (its other +5
is the declared `main_c.zig` overlap, N190). The sweep over the whole tree is N190.

**state**: CLOSED by task tail-s1 (`LOG-PORT-ONLY.tsv`, 48 rows at close).

## N82 — the port's hyperlink hash is SipHash where the reference's is Wyhash (observable through the PSL arm)

- **id**: N82. Not in the inventory. Predates this round.
- **found by**: the R13-X2 review (recorded by task R13-X3); the UD131 PSL route is what makes the
  difference observable.
- **severity**: low. **kind**: R1 fidelity candidate (a divergence from the reference that is in principle
  observable, not yet shown to break a user-facing behaviour).
- **oracle**: `src/terminal/hyperlink.zig` — the `hyperlink.Set` context hashes a hyperlink with
  `std.hash.Wyhash.init(0)`.
- **rustty**: `crates/rustty-vt/src/hyperlink.rs:376` uses `std::collections::hash_map::DefaultHasher`
  (SipHash-1-3). Its PORT NOTE argues the hyperlink hash is never oracle-compared and that "any
  self-consistent hash is parity-preserving".

That argument is what UD131 undercuts. The ref-counted set's PSL-overflow arm (`ref_counted_set.zig:268`)
turns the hash into observable behaviour: WHICH inputs collide in their low bits decides which inputs
drive the set to `OutOfMemory`, and so which inputs the reference crashes on (UD131) or the port degrades.
For STYLES the port's hash matches the reference (`std.hash.int`, MEASURED: the 32 UD131 colours cluster
identically in both), so the crash/degrade inputs coincide. For HYPERLINKS they do not: SipHash and
Wyhash cluster different URI sets, so a hyperlink-ceiling PSL trigger reachable on the reference
(`hyperlink_bytes` is `u16`, so the map has a reachable ceiling) would need a DIFFERENT set of URIs on the
port, and vice versa. No user-visible divergence has been constructed — the sets are internal and the port
degrades safely where the reference would crash — so this is a candidate, not a confirmed R1 break.

**evidence**: UD131 (the style hash matches, MEASURED; the mechanism by which an unseeded set hash becomes
observable); `hyperlink.rs:376` vs `hyperlink.zig` (`DefaultHasher` vs `Wyhash.init(0)`).

**proposed fix** (NOT applied here): if adopted as R1, port the hyperlink context hash as Wyhash (as N88's
style-set work aligned the style item layout), so the port and reference cluster hyperlinks identically.
Not fixed in this round; recorded for adjudication.

**state (before tail-s2)**: OPEN (R1 candidate).

**Closed by task tail-s2 (2026-09-23)**: fixed per R1. `PageEntry.hash`
(`crates/rustty-vt/src/hyperlink.rs`) now produces the reference's 64-bit value.

- **Wyhash**: the port already existed (`crates/rustty-zigstd/src/wyhash.rs`, used by the page hash
  maps); its body matches Zig 0.16.0's `lib/std/hash/wyhash.zig` line for line. Its tests now carry all
  five of Zig's `wyhash.zig` tests (`smhasher` and `iterative api`, with `verify.zig`'s helpers, were
  missing; `test vectors at comptime` has no Rust compile-time form and is noted, not ported). A
  differential against a Zig 0.16.0 probe: 602 random inputs, lengths 0-300 (each twice), seed 0 then
  random seeds, one-shot and iterative in random 1-64-byte chunks. MEASURED: 602/602 one-shot and
  602/602 iterative hashes equal the probe's, before and after this change (the Wyhash port was already
  exact). 25 of the rows are kept in `differential_against_zig`.
- **What the reference hashes** (`hyperlink.zig:143`, `lib/std/hash/auto_hash.zig`): the tag as one
  byte in the union's declaration order (`explicit` 0, `implicit` 1; the port's `PageEntryIdTag`
  discriminants are the reverse, so the bytes are literals (superseded by the S-2b follow-up below)); `.implicit` as its 4 native bytes; each
  `.Deep` slice (`.explicit` id, then `uri`) as its bytes FOLLOWED BY its `len` as a native `usize`.
  The old port also dropped the two lengths, so `(id "ab", uri "x")` and `(id "a", uri "bx")` hashed
  alike; they no longer do.
- **Differential through the reference's own `hash`** (a probe built from `20abdb50a`'s `src/` with
  `Page.insertHyperlink`, `hyperlink.Set.get`, `PageEntry.hash` and the set context's `hash`): 600
  random entries (tag 50/50, ids 1-40 bytes, URIs 1-300 bytes, raw bytes) plus 14 readable ones.
  MEASURED: before, 0/600 matched; after, 600/600 and 14/14 match on both `entry.hash` and the
  context's `hash`.
- **Bucket placement**: both sets take the bucket as `(hash +% i) & table_mask`
  (`ref_counted_set.zig:569`, `:629`; `ref_counted_set.rs:1223`, `:1318`), so the same hash gives
  the same placement. MEASURED: 16 insert sequences (set counts 4/8/16/32, 4 seeds each, each run
  past capacity) and one readable count-16 sequence: before, all 16 random sequences differed from the
  reference in table or PSLs; after, every add result (id or `SetOutOfMemory`), the whole table, every
  item's bucket/PSL/refcount, `max_psl`, `living` and `next_id` are identical. The R13-L item stride is
  unchanged (`@sizeOf(hyperlink.Set.Item)` 48 on the probe).
- **Performance**: the reference's shape (six short `update`s on a streaming hasher) measured 3x the
  one-shot `Wyhash.hash` on OSC 8-sized entries and a paired +4..7% on an OSC 8-heavy
  `+terminal-stream` corpus, so an entry of at most 256 bytes of hash input is gathered into a stack
  buffer and hashed one-shot (the same value: Wyhash's iterative API is concatenation-exact, and both
  paths are in the differential and in the tests). Numbers in the task tail-s2 report; measured under
  load, so PROVISIONAL.
- **Tests**: `hyperlink::page_entry_hash_tests` (`hash_matches_the_reference`: 14 probe values, including
  both sides of the 256-byte edge and the two "abx" entries; `set_placement_matches_the_reference`: the
  count-16 table and PSLs); `wyhash::tests::{test_smhasher, test_iterative_api,
  differential_against_zig}`. Negative controls (one at a time, each failing for its stated reason):
  lengths dropped, tag bytes swapped, the streaming path's seed changed (fails only on the long rows),
  Wyhash's `total_len` dropped from `final2`, `total_len` skipped on buffered `update`s.

The ledger row for the same finding, `S155-F1` (DISPUTED, WAIVED as not observable), is not this task's
row and is left as it is; its premise ("any self-consistent hash preserves every observable result") no
longer describes the code. (Superseded by the S-2b follow-up below: the row was handed to this task and
re-marked CLOSED.)

**Follow-up, task tail-s2 (S-2b)**: the port's `PageEntryIdTag` discriminants were the reverse of the
reference's tag (`implicit` 0). They now follow the reference (`explicit` 0, `implicit` 1), so the tag
byte stored in the page mapping matches: MEASURED with the same probe, byte 16 of `PageEntry.Id` is 01
for an implicit link and 00 for an explicit one, and an all-zero id is `.explicit`. (That covers live
entries and a fresh, zero-filled set slot. A reaped slot is written with `Item::default()`, whose
`PageEntry::default()` is `.implicit` 0, so it is not byte-identical to a fresh one; the reference leaves
`Item.value` `undefined` in both cases (`ref_counted_set.zig:85`) and never reads it, so neither has
reference-defined bytes and the default is left as it was.)
(`hyperlink::page_entry_tag_byte_tests`; negative control: the old numbering fails both tests). No C
header, snapshot format (it has its own `Kind` byte) or compression golden depends on the numbering. The
ledger row S155-F1 is re-marked CLOSED with this evidence. The probe also found N208 (the payload
slice's field order).

**state**: CLOSED (task tail-s2).

## N100 — nine more `std.fmt.parseInt` sites in `src/terminal` parsed with Rust's `str::parse`

- **id**: N100. Not in the inventory.
- **found by**: batch T-A (tail wave), while closing S170-F1 / S186-F1 / S194-F3 / S195-F2 / S197-F1,
  by grepping the reference's `src/terminal` for every `std.fmt.parseInt` / `parseUnsigned` call and
  reading each port counterpart.
- **severity**: low. **kind**: behavioral-difference (same class as the five rows).
- **oracle**: `src/terminal/osc/parsers/osc9.zig:28, 196` (`parseUnsigned`);
  `src/terminal/tmux/layout.zig:99, 113, 127, 141, 163`; `src/terminal/tmux/output.zig:194-204`;
  `src/terminal/tmux/viewer.zig:1031`; `src/terminal/tmux/control.zig:178-180` (plus the regex
  id captures at `:234-470`, which only ever see `[0-9]+`).
- **rustty**: `crates/rustty-vt/src/osc/parsers/osc9.rs` (sleep and progress arms),
  `crates/rustty-vt/src/tmux/{layout,output,viewer,control}.rs`.

Zig's `std.fmt.parseInt` accepts interior `_` digit separators and `-0` for unsigned types, and
`parseUnsigned` rejects any sign byte; Rust's `str::parse` rejects `_` and `-0` and accepts a leading
`+`. None of these sites belonged to a ledger row. The five rows' sites and these now all go through one
port of the std function, `crates/rustty-vt/src/zig_parse_int.rs`, which carries the std test table
(`std_fmt_parse_int_table`). Observable before the fix: `OSC 9;1;4_20` slept 100 ms instead of 420,
`OSC 9;1;+420` slept 420 instead of 100, a tmux `%end 1_0 2 0` guard line did not end its block, a
`list-panes` `pane_tabs` entry `1_0` set no tab stop, and `output.rs` reported `-a` as `Overflow`
where the reference reports `InvalidCharacter`.

Checked and left unchanged: `kitty_desktop_notification.rs` (`w=`) filters `_` first, as the reference
does, and its remaining forms agree; `context_signal.rs` accepts only `[0-9]` bytes before parsing;
`x11_color.zig`'s calls run over the built-in `rgb.txt`, not input; `color.zig:65`/`:663` already have
Zig-faithful ports in `crates/rustty-vt/src/color.rs`.

**tests**: `osc_9_numbers_use_zig_parse_unsigned`, `numbers_use_zig_parse_int` (layout),
`parse_integer_variables_use_zig_parse_int` (output), `pane_state_tab_stops_use_zig_parse_int`
(viewer, `--features oniguruma`), `block_terminator_numbers_use_zig_parse_int` (control). Each failed
before its site's change, on the `_`-separated input.

**state**: FIXED (batch T-A).

**update (tail-a2, extended R13)**: the two osc9 arms now refuse a `_` before `parseUnsigned` (UD141,
following Ghostty's own OSC 99 filter), so `osc_9_numbers_use_zig_parse_unsigned` now asserts that
`9;1;4_20` sleeps the default 100 ms and `9;4;1;4_2` has no progress. The tmux sites stay faithful
(their input is tmux's own generated output; see UD141 "port handling").

## N101 — the a=f frame's own RGBA conversion could not answer ENOMEM

- **id**: N101. Not in the inventory.
- **found by**: batch T-A (tail wave), while closing S190-F2, by reading every caller of
  `pixel.rgbaFromFormat`.
- **severity**: low. **kind**: missing-error-path (same class as S190-F2).
- **oracle**: `src/terminal/kitty/graphics_exec.zig:600-612` (`pixel.rgbaFromFormat(...) catch
  |err| { encodeError(&result, err); return result; }` for a non-RGBA frame image);
  `src/terminal/kitty/graphics_pixel.zig:18-40` (`Allocator.Error![]u8`).
- **rustty**: `crates/rustty-vt/src/kitty/graphics_exec.rs` (frame load, the `frame_img.format
  != Rgba` block); `crates/rustty-vt/src/kitty/graphics_pixel.rs` (`rgba_from_format`).

S190-F2 cites only `convertImageToRgba`, but its one failure is `rgbaFromFormat`'s allocation,
and the frame load calls `rgbaFromFormat` directly on the frame image just before it. The port's
`rgba_from_format` was infallible (`Vec::with_capacity`, `to_vec`), so this arm was carried as a
comment and an allocation failure aborted instead of answering `ENOMEM: out of memory`.
`rgba_from_format` now returns `Result<Vec<u8>, OutOfMemory>` through `try_reserve_exact` (both
the scalar and the wuffs arms), and all three callers answer ENOMEM.

**tests**: `kittygfx_animation_frame_rgba_conversion_oom_is_enomem` (the frame's conversion, then
the image's) and `kittygfx_animation_compose_rgba_conversion_oom_is_enomem`, through the
`cfg(test)` fail point `RGBA_FROM_FORMAT_TW`. Both failed with the ENOMEM arms disabled.

**state**: FIXED (batch T-A).

## N102 — tmux control-mode parsers raced Oniguruma's lazy global init (the intermittent `tmux::` test crashes)

- **id**: N102. Not in the inventory. Reported by the T-A reviewer (review-T-A "Pre-existing"),
  investigated and fixed by the T-A fix round (tail-a2).
- **severity**: medium (heap corruption; test-harness-visible, and reachable by any multi-threaded
  host of rustty-vt that has not initialized Oniguruma). **kind**: concurrency / C-init race.
- **what**: `cargo test -p rustty-vt --lib --features oniguruma tmux::` crashed the test binary
  intermittently in parallel mode: MEASURED 27 of 1,500 runs at tail-a2 before the fix (SIGTRAP 20,
  SIGABRT 6, SIGSEGV 1), 0 single-threaded per the reviewer. Under lldb the faulting thread was
  `tmux::control::tests::tmux_client_session_changed` in `libsystem_malloc` `mfm_free` (a malloc
  assertion), called from `callout_func_list_add` (`regparse.c:1340`) <- `onig_set_callout_of_name`
  <- the ASCII encoding's `init` (`ascii.c:64`) <- `onig_initialize_encoding` <- `onig_initialize`
  (`regcomp.c:7773`) <- `onig_reg_init` (`regcomp.c:7671`) <- `onig_new` <- `oni::Regex::init` <-
  `Parser::parse_notification_oni`. `onig_new` initializes Oniguruma's process-global tables lazily
  on first use, with no synchronization, so several parser threads reaching their first regex
  together raced inside `onig_initialize`. The reference never runs that path concurrently: the app
  calls `oni.init(&.{oni.Encoding.utf8})` at startup (`global.zig:215`; the port's
  `rustty-apprt` does the same through `oni::testing::ensure_init`), `zig test` is serial, and
  libghostty-vt is built without Oniguruma. Same class as the earlier `ensure_init` race (memory
  note "parallel cargo test exposes C init races").
- **fix**: `Parser::parse_notification_oni` (`crates/rustty-vt/src/tmux/control.rs`, PORT NOTE N102)
  calls `oni::testing::ensure_init()` (a `OnceLock` around the same UTF-8-only `onig_initialize`)
  before any regex; after the app's own call it is a no-op, and an init error still surfaces through
  the reference's `regex init failed` path. A stale note there that said the `oni` binding crate did
  not exist was corrected.
- **tests**: `tmux::control::tests::concurrent_first_notifications_do_not_race_oniguruma_init`
  (parent/child: 16 threads parse their first notification together in a fresh child process, 40
  times). Negative control: without the fix the first child aborted (SIGABRT). After the fix, 1,500
  parallel runs of `tmux::`: 0 failures.
- **state**: FIXED (tail-a2, 249d174a).

## N110 — `+version`'s commit hyperlink came from a separate `git rev-parse`, not from the version's `+BUILD`

- **id**: N110. Not in the inventory. Found by the review T-B fix round (tail-b2) while moving the
  git-derived version into its own crate.
- **severity**: low. **kind**: behavioral-difference (`+version` output on a TTY).
- **what**: the reference's `+version` wraps its header in an OSC-8 link to
  `.../commit/<build_config.version.build>` (`src/cli/version.zig`), where `version.build` is the
  `+BUILD` metadata of the composed version (`Config.zig:290-338`): the short hash for an untagged
  checkout, `0000000` without git, `null` (no link) for a `vX.Y.Z`-tagged release, and whatever an
  explicit `-Dversion-string` carries. `rustty-cli` captured the hash separately in its build script
  (`git rev-parse --short HEAD`, `RUSTTY_COMMIT_HASH`), so a tagged release was linked, a build
  without git was not, and an explicit `RUSTTY_VERSION_STRING`'s build metadata was ignored.
- **fix**: `rustty-cli` `commit_hash()` returns `rustty_version::version_build()`, the `+BUILD` of the
  composed `VERSION_STRING`; the separate capture and its HEAD watch are gone. Test
  `version::tests::commit_hash_is_the_version_build_metadata` (hash == the version's `+BUILD`, a real
  hex hash in a git build). `rustty-version` `tests::version_is_derived_from_git` checks
  `version_build()` per arm (git: the short hash; tagged: `None`; no git: `0000000`).
- **state**: CLOSED (tail-b2).

## N120 — two ports of `std.fmt.parseFloat`: rustty-config's is f64-only and private

- **id**: N120. Not in the inventory.
- **found by**: task T-C (tail wave), closing S67-F4.
- **severity**: low (duplication; no behaviour difference found). **kind**: maintenance / drift risk.
- **oracle**: Zig 0.16.0 `lib/std/fmt/parse_float.zig` + `parse_float/{parse,FloatStream,convert_hex}.zig`.
- **rustty**: `crates/rustty-config/src/value_types.rs` (`pf_*`, `parse_float_f64`, parity S75-F1; plus
  `zig_float_strip_underscores` / `parse_float_f32`) and, since T-C, `crates/rustty-args/src/std_fmt.rs`
  (`parse_float::<f32|f64>`, `parse_int`).

S67-F4 needed a `std.fmt.parseFloat` port in `rustty-args` (`parse_field_float`, the `f32`/`f64` arm of
`parseIntoField`). `rustty-config` already had one, for `f64` only and private to `value_types.rs`, and it
sits ABOVE `rustty-args`, so `rustty-args` cannot call it. T-C ported the grammar again in `rustty-args`
(generic over `f32`/`f64`, `convert_hex` included) rather than edit `value_types.rs`, which belongs to the
config area's batch. The two copies now implement the same grammar. `parse_float_f32` in `value_types.rs`
is NOT the full grammar: it validates underscores and strips them, then calls Rust's parser, so it rejects
hex floats. Whether any `f32` value in `value_types.rs` reaches `parse_float_f32` with a hex float is
UNVERIFIED.

**proposed fix** (NOT applied here): make `value_types.rs` call `rustty_args::std_fmt::parse_float` and
delete its `pf_*` copy and `parse_float_f32`, in the config area.

**addendum (T-C rebase onto T-E, 2026-09-23)**: T-E (S78-F2, `549b63fd`) added a third partial port,
of `std.fmt.parseInt` for unsigned types with base 0: `value_types.rs` `parse_int_u8_base0` /
`parse_unsigned_base0`. `rustty_args::std_fmt::parse_int` is also a port of that function (base 0 included),
so the same consolidation applies to it. Not compared value by value; UNVERIFIED whether the two agree
on every input.

**fix round T-C3 (2026-09-23)**: rustty-config depends on rustty-args (`crates/rustty-config/Cargo.toml`),
so the dependency direction allows the switch, and it is done. `value_types.rs` now calls
`rustty_args::std_fmt` for every `std.fmt` integer and float parser it ported, and those copies are deleted (the u21 hex parser is still a local copy; it matches Zig on every input the T-C3 review probed):
- `pf_*` + `parse_float_f64` -> `std_fmt::parse_float::<f64>` (`Config.zig:6445, 9289` and the
  `MouseScrollMultiplier` fields).
- `zig_float_strip_underscores` + `parse_float_f32` -> `std_fmt::parse_float::<f32>` (`Config.zig:9481`).
  This one CHANGES behaviour: the old copy used Rust's grammar after stripping `_`, so it rejected hex
  floats. That was a port bug; see N123.
- T-E's `parse_unsigned_base0` / `parse_int_u8_base0` -> `std_fmt::parse_int::<T>(.., 0)` (`limit.zig:28`,
  `Config.zig:9878`). Same accept/reject set by reading (the old copy accumulated in `u128` and cast at
  the end; every caller collapses both errors into `InvalidValue`).
- `parse_int_u32_base10` -> `std_fmt::parse_int::<u32>(.., 10)` (`Config.zig:9472, 10339, 10341, 10345`).
- `parse_unsigned_u64_base10` -> `std_fmt::parse_unsigned::<u64>(.., 10)` (`Config.zig:10222`). The old
  copy rejected every `_`. Its only caller stops at the first prefix that fails, and a prefix ending in
  `_` always fails, so no `_` reaches a successful parse. The results are the same on every input.
- `value_types::ParseIntError` is now a re-export of `std_fmt::ParseIntError` (same variants).

Evidence: all 261 pre-existing rustty-config lib tests pass unchanged, among them the 55-row
`parse_float_f64_matches_std_fmt_parsefloat` Zig oracle table and the `WindowPadding` / `Limit` /
`quick-terminal-size` integer-spelling tests. `std_fmt`'s own tests are Zig 0.16.0's std tables
(parseInt, parseUnsigned, "parse u64 digit too big", parseFloat, "nan and inf", "largest normals",
hex.*), verbatim for every Rust-representable type. The `u21` hex parses (`Config.zig:8293, 8407`)
have no Rust type and were not touched.

**Remaining (follow-up, not done here):** rustty-vt sits below rustty-args and has its own partial
copies (`crates/rustty-vt/src/color.rs` `parse_int_u8` / `parse_unsigned_u8`,
`crates/rustty-vt/src/kitty/graphics_command.rs` `parse_int_i32` / `parse_int_u32`, and a
`zig_parse_int.rs` that another task is preparing and is not on `tail-c3`). All of them, and
`rustty_args::std_fmt`, should become ONE port in a shared crate at the lowest layer (below rustty-vt)
that every crate calls, with `std_fmt`'s std-table tests moved with it.

**state**: CLOSED (task tail-s9, 2026-09-23): one port, in `rustty_zigstd::fmt`; see N150's closing entry for
the comparison, the moves and the test merge. The rustty-config part was done by T-C3; tail-s9 moved rustty-config
(including the `u21` hex parser, now `parse_int_type(IntType::unsigned(21), .., 16)`), rustty-args, rustty-cli and
rustty-vt onto `rustty_zigstd::fmt` and deleted `rustty_args::std_fmt`.

## N121 — five `+show-face` error lines print a Rust Debug value where the reference prints `error.<Name>`

- **id**: N121. Not in the inventory.
- **found by**: task T-C (tail wave), closing S70-F6, whose audit text mentions these sites ("Similarly
  `show_face.rs` prints `{err:?}`") although its cited anchors are `ssh.zig:281-283, 530-545` only.
- **severity**: low (stderr text on failure paths). **kind**: behavioral-difference (message text).
- **oracle**: `src/cli/show_face.zig` `runArgs`/`lookup`: `Unable to load config: {}` (:145), `Unable to
  initialize font grid set: {}` (:164), `Unable to get font grid: {}` (:184), `Unable to get face: {}`
  (:226), `Unable to get name of face: {}` (:233), at 20abdb50a.
- **rustty**: `crates/rustty-cli/src/show_face.rs`, the five `write!(stderr, "Unable to …: {err:?}")` sites,
  each carrying a PORT NOTE naming this entry.

Zig's `{}` of an error value prints `error.<Name>` (MEASURED, Zig 0.16.0). The port prints the Debug of its
own error types. For three of them the names cannot be produced by a prefix alone: `LoadError` carries an
`io::Error`, and `collection::Error::Face(FaceError)` and the face-name errors carry backend payloads, so
each needs a mapping to the reference error set of the call (`Config.load`, `SharedGridSet.ref`,
`Collection.getFace`, `Face.name` per backend), which T-C did not derive. T-C fixed the two sites whose
reference error is fixed: `Error parsing args: {}` (via `zig_error::args_error`, with S69-F5) and
`Unable to parse string as unicode: {}` (`Utf8View.init`'s only error, `error.InvalidUtf8`).

**proposed fix**: map each port error type to the reference's error names (as `zig_error::load_error`
does for `Config.loadFile`), and print through that.

**state**: **CLOSED** (G-1, 2026-09-28). The five sites print Zig's `{}` of the error. `Unable to load config` goes through `zig_error::load_error` (the `Config.load` names). The other four print `rustty_zigstd::error_name::ZigError(&err)`, a new shared mechanism (`crates/rustty-zigstd/src/error_name.rs`: trait `ErrorName` giving `@errorName`, and `ZigError` rendering `error.<Name>`). `rustty-font-shaper` implements it for the error types those calls return: `ft::FtError`, `fc::Error`, the three backend `FaceError`s, `collection::Error`, the four `discovery::Error`s, `library::LibraryError` (uninhabited), `shared_grid::InitError` and `shared_grid_set::Error`. Leaves use their variant names (the reference's, documented at each enum); wrapper variants (`Face(..)`, `Collection(..)`, `Discovery(..)`, `GridInit(..)`, `Fc(..)`, `Freetype(..)`) return the wrapped error's name, since a Zig error union is flat. `Face.name`'s reference set is `Allocator.Error`, so that line prints `error.OutOfMemory`. Built for the default (coretext), coretext_freetype, fontconfig_freetype, freetype and wasm32 (web_canvas) backends on macOS. Test: `shared_grid_set::error_name_tests::nested_errors_print_the_innermost_zig_name` (nested chains and leaves) plus `rustty-zigstd` `error_name::tests`. The sites themselves are not driven: none of the four failures can be produced from a test without a fault-injection seam (not added). Negative control: the collection `Face(..)` arm removed (Debug fallback) fails with `error.Face(OutOfMemory)`. Was: **OPEN** (triage T-1, 2026-09-28). Unchanged: the five `write!(stderr, "Unable to …: {err:?}")` sites in `crates/rustty-cli/src/show_face.rs` (:369, :426, :464, :581, :598). Left: map each port error to the reference's error names; `Unable to load config` can reuse `rustty_config::zig_error::load_error` (moved there by T-E/S-15), the grid-set, grid, face and face-name sites need new mappings per backend. Effort S-M, no long run; observable (user-visible `+show-face` stderr text on failure paths, R1). Was: OPEN.

## N122 — `+new-tab` accepted a surface id with a leading `+`

- **id**: N122. Not in the inventory. Found by the T-C reviewer (review-T-C2, "New finding"), fixed by the
  T-C3 fix round.
- **severity**: low (CLI argument and environment validation). **kind**: behavioral-difference.
- **oracle**: `src/cli/new_tab.zig:69` (`--surface-id=`: `std.fmt.parseUnsigned(u64, trimmed, 0)`) and
  `:251` (the `GHOSTTY_SURFACE_ID` fallback: `std.fmt.parseUnsigned(u64, e, 0)`), at 20abdb50a.
  Zig 0.16.0 `parseUnsigned` calls `parseIntWithSign(T, u8, buf, base, .pos)` directly, with no sign
  strip, so a `+` reaches `charToDigit` and is `error.InvalidCharacter`. std's own `test parseUnsigned`
  asserts `parseUnsigned(u8, "+0", 10)` is `error.InvalidCharacter`. MEASURED with Zig 0.16.0:
  `parseUnsigned(u64, "+5", 0)` is `error.InvalidCharacter`.
- **rustty**: `crates/rustty-cli/src/new_tab.rs` `parse_unsigned_u64_base0`, a hand-written copy whose
  comment said "parseUnsigned == parseIntWithSign(.pos): a leading '+' is accepted", stripped the `+`.
  So `--surface-id=+5` was accepted, and so was a `+5` env value, where the reference prints
  `Error parsing args: error.InvalidCharacter` or falls back to surface 0.
- **fix**: `parse_unsigned_u64_base0` now calls `rustty_args::std_fmt::parse_unsigned::<u64>(buf, 0)` (new,
  a port of `parseUnsigned`), and the local `ParseIntError` is the `std_fmt` one. Test
  `new_tab::tests::surface_id_rejects_a_leading_plus_like_parse_unsigned`. Negative control: the `+` strip
  reinstated -> the run gets past argument parsing ("+new-tab is not supported on this platform.") where
  `Error parsing args: error.InvalidCharacter` is expected.
- **state**: FIXED (T-C3).

## N123 — `quick-terminal-size` rejected hex-float percentages

- **id**: N123. Not in the inventory. Found by the T-C3 fix round while consolidating N120.
- **severity**: low. **kind**: behavioral-difference (a config value the reference loads was rejected).
- **oracle**: `src/config/Config.zig:9481` (`QuickTerminalSize.Size.parse`: `std.fmt.parseFloat(f32,
  input[0 .. input.len - "%".len])`), at 20abdb50a. MEASURED with Zig 0.16.0: `parseFloat(f32, "0x1p4")`
  is 16, `"0x1.8p5"` is 48, `"1_0"` is 10.
- **rustty**: `crates/rustty-config/src/value_types.rs` `parse_float_f32` validated and stripped `_`, then
  called `str::parse::<f32>()`, which has no hex-float grammar, so `quick-terminal-size = 0x1p4%` was an
  `InvalidValue` diagnostic and the setting was discarded.
- **fix**: `parse_float_f32` calls `rustty_args::std_fmt::parse_float::<f32>` (N120). Test
  `value_types::tests::quick_terminal_size_percentage_uses_the_parse_float_grammar`. Negative control: the
  strip-then-`str::parse` body reinstated -> `0x1p4%` gives `Err(InvalidValue)`. A second change, also
  towards the reference (found by the T-C3 review's differential): `-nan%` used to store a NaN with the sign
  bit set (`0xffc00000`) and now stores `0x7fc00000`, as Zig does.
- **state**: FIXED (T-C3).

## N124 — `std_fmt::parse_int` named the wrong error when an overflow came before a bad character

- **id**: N124. Not in the inventory. Found by the T-C reviewer (review-T-C2, blocking issue 1), in T-C's
  own port; fixed by the T-C3 fix round.
- **severity**: low (error text in a log line). **kind**: behavioral-difference (error identity).
- **oracle**: Zig 0.16.0 `lib/std/fmt.zig` `parseIntWithSign`: it accumulates in `Accumulate` (the result
  type widened to at least 8 bits) and returns `error.Overflow` from the `math.mul` / `math.add` /
  `math.sub` that leaves that range, before it reads the rest of the input. MEASURED with Zig 0.16.0:
  `parseInt(i64, "99999999999999999999x", 10)`, `parseInt(u8, "300x", 0)`, `parseInt(i8, "300x", 0)`,
  `parseInt(i8, "-129x", 10)`, `parseInt(u32, "-1x", 10)` and `parseInt(u8, "0x100x", 0)` are all
  `error.Overflow`.
- **rustty**: `crates/rustty-args/src/std_fmt.rs` accumulated in `i128` and range-checked only at the end,
  so each of those gave `InvalidCharacter`. Visible in `ssh_cache/entry.rs`: `Invalid timestamp in cache
  entry: 99999999999999999999x err=error.InvalidCharacter` where the reference logs `err=error.Overflow`.
  The PORT NOTE that said only the variant order could differ, "and both variants are errors", was wrong.
- **fix**: `parse_int_with_sign` range-checks against `T` after every step and follows the reference's
  first-negative-digit branch. That is exact because `Accumulate == T` for every Rust integer type.
  Tests `std_fmt::tests::parse_int_overflows_before_a_later_bad_character` and
  `ssh_cache::entry::tests::cache_entry_timestamp_overflow_is_named_before_a_later_bad_character`.
  Negative control: the end-of-parse check restored -> the first fails with `InvalidCharacter` for
  `Overflow`.
- **state**: FIXED (T-C3).

## N130 — the GTK apprt decodes title BYTES lossily at every GObject-string boundary, where the reference passes them raw

- **id**: N130. Not in the inventory.
- **found by**: task T-D (tail wave), closing S57-F3 (WAIVED, pointing here).
- **severity**: low. **kind**: R1 fidelity gap with a Rust-type constraint; recorded, not fixed.
- **oracle**: the reference carries titles as `[:0]const u8` and hands the bytes to G_TYPE_STRING
  properties and GTK setters unconverted: `src/apprt/gtk/class/tab.zig:496-509` (`closureComputedTitle`,
  the config `title`), and the apprt `set_title` / window-title paths.
- **rustty**: `String::from_utf8_lossy` at `crates/rustty-apprt/src/gtk/class/tab.rs:362-366`
  (`computed_title`, config `title`), `class/window.rs:312` (window title from config),
  `class/surface.rs:1959` (the notification title), `class/application.rs:4772, 4799, 4831` (the
  `set_title` / prompt-title actions, whose bytes come from the VT stream, OSC 0/2). The S57-F3 audit
  said "all other title sources are already UTF-8 GTK strings"; the four `application.rs`/`surface.rs`
  sites show that is not so (the OSC title is raw VT bytes).

What differs, MEASURED where stated. A non-UTF-8 title reaches GTK as U+FFFD in the port and as its raw
bytes in the reference. What the reference's bytes then do is GTK's business: Pango, which renders a tab
or window label, logs `Pango-WARNING: Invalid UTF-8 string passed to pango_layout_set_text()` and keeps the
invalid byte as `0xff` (MEASURED, Pango 1.57.1 on the VM: `caf\xe9!` → `63 61 66 ff 21`), so the user sees a
replacement glyph in both, drawn differently. Other consumers of the property values (accessibility over
D-Bus, the window-manager title) were NOT measured.

Why this is not fixed row by row. Carrying non-UTF-8 in these properties needs every Rust reader of them to
avoid glib-rs's string types: `Value::get::<String>()` panics on non-UTF-8 ("Invalid UTF-8"), and
`GStr`/`GString` built from a C pointer assume UTF-8. The tab `title` property alone is stored as
`RefCell<Option<String>>` (`tab.rs:84`, via the `Properties` derive) and read back by `prompt_tab_title`
(`tab.rs:263`) into the title dialog; the window and surface titles have the same shape. Changing one site
(S57-F3) while the others stay lossy would make the chain inconsistent, and changing the property storage
is a cross-file change with crash risk (a missed reader panics on user input). S56-F3 shows the contained
version of the same fix (a raw `G_TYPE_STRING` `GValue` and a raw reader, `surface.rs`
`raw_string_value`/`raw_string_bytes`) for a signal with ONE receiver.

**proposed remedy** (NOT implemented): store the title properties as raw bytes behind hand-written
G_TYPE_STRING pspecs (set/get through `g_value_set_string`/`g_value_get_string`), route every Rust reader
through `raw_string_bytes`, and pass GTK setters the raw C string, then close this finding site by site with
a non-UTF-8 test per reader.

**state**: **OPEN** (task G-2, 2026-09-28; scoped, not attempted: the inventory below exceeds the ~3 h cap the dispatch set, and a partial change would leave the chain inconsistent, as the entry says). Precise remaining scope, port lines at `1835f6bf`, reference at the pin. (1) **Storage**, 5 `RefCell<Option<String>>` properties that must hold raw bytes behind a `G_TYPE_STRING` pspec (a raw-bytes newtype implementing `StaticType` = `G_TYPE_STRING`, `ToValue`/`FromValue` through `g_value_set_string`/`g_value_get_string`, `HasParamSpec`, so the `Properties` derive keeps working): `class/surface.rs:339` `title` and `:344` `title-override` (`surface.zig:271-278`, `privateStringFieldAccessor`), `class/tab.rs:95` `title` and `:101` `title-override` (`tab.zig:119-126`), `class/window.rs:101` `title-override` (`window.zig:1328`). (2) **Writers** that decode today: `class/application.rs:5242` (`set_title`, OSC 0/2 bytes), `:5269` (`set_tab_title`), `:5301` (`set_window_title`), `class/window.rs:347` and `class/tab.rs:408` (config `title`). (3) **Rust readers**, each must take the raw bytes: the template callbacks `tab.rs:389` `computed_title` (`tab.zig:482`, bound in `tab.blp:11-15` over the surface `title`/`title-override`) and `window.rs:2427` `closure_computed_title` (`window.zig:1368`, `window.blp:43`), whose `String` arguments are converted by glib-rs; `surface.rs:2121` `get_title`, `:2131` `get_effective_title`, `:2144` `copy_title_to_clipboard` (already sends `as_bytes()`), `:2177` `prompt_title`; `tab.rs:287` `prompt_tab_title`; `window.rs:1927` `prompt_window_title`; `command_palette.rs:842` `effective_title` (and the palette's own `title` property, `command_palette.zig:492`); the `TitleDialog` `initial-value` property (`title_dialog.rs:144`, `Option<&str>`), which feeds a GTK entry. (4) **GTK setters** that take `&str` and need the raw C string: `gio::Notification::new` at `application.rs:4201` and `surface.rs:2232` (notification titles; bodies at `:4204`/`:2235` are the same shape). (5) **Soundness hazard found while scoping (verified in the glib-rs source, not by a run)**: glib 0.22.8 `GStr::from_ptr` (`gstring.rs:143-147`) wraps a C string with `from_utf8_with_nul_unchecked`, and `GString: FromGlibPtrNone` goes through it (`:2113-2120`), so a gtk4-rs getter that returns a `GString` over a raw non-UTF-8 title, e.g. `Window::title()` at `window.rs:1927` and the test at `:2865`, would be UB, not a panic. The remedy must therefore replace those getters with a raw FFI read (`g_object_get_property` into a `GValue` read by `raw_string_bytes`, the S56-F3 helper), not only the property storage. Tests: one non-UTF-8 test per reader (display tests on `rvm`), plus a negative control per class. Effort L (about 20 sites over 6 files plus the newtype). Observable only for a non-UTF-8 title; R1. Was: **OPEN** (triage T-1, 2026-09-28). Unchanged (S57-F3 WAIVED to it). Left: the remedy written above, raw-byte title properties behind hand-written `G_TYPE_STRING` pspecs across `tab.rs`, `window.rs`, `surface.rs` and `application.rs`, every Rust reader through `raw_string_bytes`, one non-UTF-8 test per reader. Effort L, no long run but needs Linux builds and tests on `rvm`; observable only for a non-UTF-8 title (both draw a replacement glyph, differently; other consumers unmeasured), R1. Was: OPEN (R1 candidate; S57-F3 WAIVED to this entry).


## N140 — the XDG helpers collapse `homedir.home`'s `error.BufferTooSmall` into `NoHomeDir`

- **id**: N140. Not in the inventory. Found by the T-E reviewer (review-T-E "S74-F1 error name"),
  recorded by the T-E fix round (tail-e2).
- **severity**: low. **kind**: behavioral-difference (error identity).
- **what**: `homedir.zig:28` (`homeUnix`) returns `error.BufferTooSmall` when `$HOME` is longer than
  the caller's buffer, and `xdg.zig:91-92` passes it on with `try homedir.home(io, environ_map, &buf)`
  over a 1024-byte buffer. The port's `crates/rustty-os/src/xdg.rs:157` does
  `homedir::home(..).map_err(|_| DirError::NoHomeDir)?`, and `DirError` has no other variant
  (the module header at `xdg.rs:10-16` records the collapse). Since S74-F1 (T-E, b7407d17)
  `loadDefaultFiles` propagates this error, as the reference does, so it is now observable:
  - `ghostty_config_load_default_files` logs `error loading config err=error.BufferTooSmall` in the
    reference (`CApi.zig:63-66`, `{}` of the error) and `error loading config err=FileLoad(NoHomeDir)`
    in the port (`crates/rustty-config/src/c_api.rs:124-125`, `{err:?}`). By reading; the port's
    error value is pinned by `crates/rustty-config/tests/load_path_errors_propagate.rs:46-52`
    (HOME longer than 1024 bytes).
  - A sibling, same cause: `theme.zig` `Location.dir` maps `error.BufferTooSmall` from `xdg.config`
    to `error.OutOfMemory` (`:51`), while the port's `theme.rs` `dir` treats every `DirError` as
    "the directory doesn't exist" (`None`). UNVERIFIED what the reference's theme loading then
    does with that `OutOfMemory`.
- **why not fixed here**: not small. It needs a `BufferTooSmall` variant in `rustty-os`'s `DirError`
  (area `src/os`, not this batch's), a matching `FileLoadError` variant and its consumers
  (`file_load.rs`, `theme.rs`'s OOM arm, `rustty-cli`, `rustty-crash`'s `InitThreadError::Dir`), and
  the log line would still differ in form, because the C-API sites print the Rust `Debug` form
  (`FileLoad(..)`) rather than Zig's `error.<Name>` for every error, not only this one.
- **proposed fix**: add `DirError::BufferTooSmall` and propagate it through `FileLoadError`; port the
  theme `.user` arm's `BufferTooSmall => OutOfMemory` mapping; and decide separately whether the
  C-API `err={}` sites should print Zig error names (as T-E's `zig_error.rs` does for config
  diagnostics).
- **state**: PARTIALLY RESOLVED by S112-F11 (T-G, 2026-09-23): `DirError::BufferTooSmall` and
  `FileLoadError::BufferTooSmall` now carry the reference's error, and
  `load_path_errors_propagate` pins it (so the "what" bullet's `xdg.rs:157` collapse and its
  `NoHomeDir` pin are historical). Still OPEN:
  - (a) the `theme.rs` `Location.dir` sibling: the reference maps `BufferTooSmall` to
    `OutOfMemory` (`theme.zig:51`), and the port returns `None`. Not fixed in T-G2 because it is
    not small: `Location::dir`, `LocationIterator::next` and `theme::open` would all become
    fallible, `Config::load_theme` / `finalize` would need an out-of-memory `LoadError` variant
    that every consumer then handles, and `rustty-cli`'s `list_themes` would need the same.
  - (b) the C-API `err=` sites print the Rust `Debug` form (`FileLoad(BufferTooSmall)`, by
    reading), not `error.BufferTooSmall`.
- **CLOSED by task S-15 (2026-09-24, N292)**, both halves:
  - (a) `theme::Location::dir` now returns `Result<Option<Vec<u8>>, theme::Error>` and maps
    `DirError::BufferTooSmall` to `Error::OutOfMemory` (`theme.zig:51` at `20abdb50a`); every other
    `xdg::config` error is still `None` (`else => return null`). `LocationIterator::next` and
    `theme::open` pass it on (`try`); `Config::load_theme` returns it as the new
    `LoadError::OutOfMemory`, so `finalize` fails as the reference's does, before any diagnostic is
    appended; `rustty-cli`'s `list_themes` folds it into `Error::OutOfMemory`. Tests:
    `tests/load_path_errors_propagate.rs::theme_user_dir_buffer_too_small_is_out_of_memory` (dir,
    next, open, finalize, and the rendered `error.OutOfMemory`) with the short-`HOME` control
    `theme_user_dir_resolves_with_a_short_home`; `list_themes::r13_flush_tests::
    a_home_too_long_for_the_xdg_lookup_is_out_of_memory` (re-executed child, env cleared, 1101-byte
    `HOME`). Negative controls, one at a time: `BufferTooSmall => Ok(None)` fails at "Location.dir
    (.user)" (`Err(OutOfMemory)` vs `Ok(None)`); `load_theme` swallowing the error fails at
    "Config.finalize must return OutOfMemory, got Ok(())"; `list_themes` swallowing it fails with
    "got Ok(1)".
  - (b) the `LoadError` rendering (`error.<Name>`) moved from `rustty-cli::zig_error::load_error`
    to `rustty_config::zig_error::load_error` (the CLI helper delegates), and the C-API sites use it:
    the three `error loading config`, `error loading config from file` and `error finalizing config`
    sites print `zig_error::load_error(&err)`, `error finding trigger` prints `error.{InvalidFormat,
    InvalidAction}`, and `error opening config in editor` renders its `FileLoadError` with
    `openFileAbsolute`'s errno table (exact for `configPath`'s open; UNVERIFIED for the
    `stat`/`openDir`/`createDirPath`/`createFileAbsolute` failures, whose Zig switches were not
    re-derived). Tests: `c_api::log_site_tests::load_file_logs_...` now pins
    `err=error.FileNotFound`; new `trigger_logs_the_zig_error_name` drives `err=error.InvalidAction`;
    the structural test requires every site to go through `zig_error` and no `err={err:?}` to
    remain. Negative control: the `load_file` site back to `{err:?}` fails both tests. Not changed:
    `path={s}` is still rendered through `from_utf8_lossy` (the `log_err` sink takes a `String`).

## N150 — a third port of Zig's number parsers, now at the lowest layer (`rustty-zigstd::fmt`)

**Recorded 2026-09-23 (task T-F).** Closing S85-F1 (`Metrics.Modifier.parse`, in `rustty-font`)
and S100-F1 (the keybind action numbers, in `rustty-input`) needed Zig 0.16.0's
`std.fmt.parseInt` / `parseUnsigned` / `parseFloat` below the config and CLI crates. They are
ported in `crates/rustty-zigstd/src/fmt.rs` (with Zig's own std tests for the Rust-representable
types, and a 133-input differential corpus checked against Zig's results), plus `format_float_d`
(Zig's `{d}` for `f32`/`f64`, checked against Zig's text for 57 values) for the `web_canvas` face
(S91-F2).

Task T-C, running in parallel, ported the same parsers into `rustty_args::std_fmt` (NEW-FINDINGS
N120), which records that the copies should be merged "into a single crate at the lowest layer".
`rustty-zigstd` is that layer (a leaf over `rustty-util`, already a dependency of `rustty-input`
and now of `rustty-font` and `rustty-font-shaper`). After both tasks merge there are two full
ports (`rustty_args::std_fmt`, `rustty_zigstd::fmt`) plus rustty-vt's partial copies.

**state (before tail-s9)**: OPEN. The N120 follow-up can move `rustty-args` and `rustty-config` onto
`rustty_zigstd::fmt` (or the reverse) and delete the other copy; the two were written
independently, so their test sets should be merged, not just one kept.

**Closed by task tail-s9 (2026-09-23)** (N120 too). ONE port of `std.fmt.parseInt` /
`parseUnsigned` / `parseFloat` remains: `crates/rustty-zigstd/src/fmt.rs`.

- **Copies found** (workspace-wide search for `parseInt` / `parseUnsigned` / `parseFloat` /
  `charToDigit` ports): `rustty_args::std_fmt` (full), `rustty_zigstd::fmt` (full),
  `crates/rustty-vt/src/zig_parse_int.rs` (integers, any width through a runtime `IntType`),
  `crates/rustty-vt/src/color.rs` (`parse_int_u8`, `parse_unsigned_u8`, `parse_unsigned_u16`,
  `parse_int_u8_with_sign`, `char_to_digit`), `crates/rustty-config/src/value_types.rs`
  `parse_int_u21_base16`. Callers already on a shared port: rustty-input, rustty-font,
  rustty-font-shaper (zigstd); rustty-config and rustty-cli (args).
- **Not merged (they are not ports of the std function, or not faithful ones; changing them is a
  behaviour change, outside this structural task)**: `crates/rustty-os/src/uri.rs`
  `parse_int_u16` / `parse_int_u8_hex` (Rust `str::parse` / `from_str_radix` standing in for
  `std.fmt.parseInt(u16, s, 10)` / `(u8, s, 16)`: by reading, both reject the `_` separators and
  the `-0` that Zig accepts; rustty-os is also T-G's area), and `crates/rustty-vaxis/src/cell.rs`
  `parse_unsigned_u8` / `char_to_digit` and `crates/rustty-vaxis/src/parser.rs` `parse_unsigned`
  (libvaxis's `std.fmt.parseUnsigned` calls; no `_` handling). Verdicts (review of tail-s9, measured
  with Zig 0.16.0): `uri.rs` IS an R1 gap — the reference is `std.Uri`, which calls `std.fmt.parseInt`
  directly, so `file://host:8_0/x` gives port 80, `:-0` gives 0 and `[::1]:1_0` gives 10 where the port
  rejects all three (reachable through OSC 7); `vaxis/parser.rs` IS an R1 gap — libvaxis 0.6.0 calls
  `parseUnsigned` / `parseInt`, so the port also wrongly rejects `+5` and `-0`; `vaxis/cell.rs` is NOT a
  gap — the reference checks the length first, so its input is always exactly two hex bytes, on which
  the two agree. Both gaps are tracked as N206. `osc/parsers/kitty_desktop_notification.rs` (OSC 99) and
  `context_signal.rs` use Rust's parser after the reference's own `_` / digits-only filter, which
  leaves no input on which the two grammars differ; `kitty/dnd_command.rs` `parse_unsigned` ports
  a reference helper, not the std function.
- **Comparison** (MEASURED): each port run against Zig 0.16.0 over a fresh probe of 3120 integer
  inputs x 22 types (`u1 u2 u4 u7 u8 u9 u16 u21 u32 u64 usize i1 i2 i3 i4 i5 i8 i16 i32 i43 i64
  isize`, each port on the types it supports) x bases 0/2/8/10/16/36 x `parseInt`/`parseUnsigned`
  (823,680 Zig results), and 1138 float inputs x f32/f64 bit patterns; the inputs include the union
  of the three ports' tables, the zigstd corpora, signs, `_` placement, prefixes, overflow before a
  bad character, non-ASCII bytes, hex floats, special values and subnormals. **No port differed
  from Zig on any input, and so no two ports differed in value.** The differences were in reach and
  in the tests: only the rustty-vt copy handled Zig's odd widths (so zigstd and args skipped the
  `i1`/`i2`/`i4`/`i5`/`i43`/`u1`/`u2`/`u7` std-table rows); zigstd lacked nothing args had; args'
  parseFloat table asserted `is_err()` where Zig says `expectError(error.InvalidCharacter, ..)`;
  zigstd's compared `approxEqAbs` in `f64` where Zig compares in `T`. A harness negative control
  (Accumulate width = result width; the `many_digits` sticky bit dropped) produced 10,287 and 1
  mismatches, so the probe detects both. Probe and harness: task tail-s9 report.
- **Fix**: `rustty_zigstd::fmt` keeps its float grammar and takes rustty-vt's arbitrary-width
  integer core (`IntType`, `parse_int_type`, `parse_unsigned_type`; `parse_int::<T>` /
  `parse_unsigned::<T>` for native `T`, `isize` included). Deleted: `rustty_args::std_fmt`, the
  rustty-vt `zig_parse_int.rs` parser (the file is now `protocol_int.rs` with only the UD141
  `reject_digit_separators` filter, unchanged, still called first at every protocol site), the
  `color.rs` copies, the config `u21` copy. Callers (rustty-args `args.rs`, rustty-config
  `value_types.rs`, rustty-cli `new_tab.rs` / `ssh_cache/entry.rs`, and 12 rustty-vt files) call
  `rustty_zigstd::fmt`; rustty-args and rustty-cli gained a `rustty-zigstd` edge (a leaf over
  `rustty-util`; `check_acyclic.sh` and `cargo_cycle_check.py` GREEN).
- **Tests**: the zigstd copy of Zig's tables is the one kept, now with the odd-width rows (so
  `test parseInt` and `test parseUnsigned` are ported in full) and `approxEqAbs` over `T`. Moved
  into it: args' `parse_int_sign_and_separator_edges`, `parse_int_overflows_before_a_later_bad_character`,
  `hex_many_digits_sticky_bit_rounds_up`; rustty-vt's `digit_separators_and_accumulate_width`;
  `color.rs`'s `parse_unsigned_u8_base10_zig_semantics`, `parse_unsigned_u16_base16_zig_semantics`.
  Removed as duplicates (twin in `crates/rustty-zigstd/src/fmt/tests.rs`): args `std_parse_int`
  (`test_parse_int`), `std_parse_unsigned` (`test_parse_unsigned`), `std_parse_u64_digit_too_big`
  (`test_parse_u64_digit_too_big`), `std_parse_unsigned_comptime` (`test_parse_unsigned_comptime`),
  `std_parse_float_f32` + `std_parse_float_f64` (`test_parse_float` + `nan_and_inf`),
  `std_largest_normals` (`largest_normals`), `std_hex_special` (`hex_special`), `std_hex_zero`
  (`hex_zero`), `std_hex_f32` (`hex_f32`), `std_hex_f64` (`hex_f64`), `std_many_digits_hex`
  (`many_digits_hex`); rustty-vt `std_fmt_parse_int_table` (`test_parse_int`),
  `std_fmt_parse_unsigned_table` (`test_parse_unsigned`). Every caller's own tests are unchanged.
- **Perf**: see the task tail-s9 report (`#[inline]` on the integer core cost `+terminal-parser`
  about 10% through a codegen-unit reshuffle and was dropped; PERF(port) note in `fmt.rs`).

**state**: CLOSED (task tail-s9).

## N151 — CoreText keeps naming a font after a variation instance of the same data was built

**Recorded 2026-09-23 (task T-F2, from the T-F review).** Test isolation, not product behaviour.
After `deferred_face::tests::load_coretext_deinits_the_face_when_set_variations_fails` (S85-F3)
builds a `wght=700` variation instance of the data-loaded embedded JetBrains Mono, CoreText can
answer `CTFontCopyFamilyName` = "JetBrains Mono" for the same font data with its `name` table
renamed away, for the rest of the process. `face::coretext::tests::
a_face_without_a_family_name_is_unknown` then fails its precondition. MEASURED on macOS: the two
nameless-font tests plus the S85-F3 test, run with `--test-threads=1`, failed 20 of 30 runs (always
the `face::coretext` one); the reviewer measured 11 of 150 full-suite runs on `tail-f` against 1 of
150 at the base, and 8 of 200 with a lock shared by the two tests, so the state outlives the test
that made it and a lock does not help.

**Fix**: both nameless-font tests (`face::coretext::tests::a_face_without_a_family_name_is_unknown`
and `deferred_face::tests::a_coretext_font_without_a_family_name_is_unknown`) run their body in a
re-executed child of the test binary (`crates/rustty-font-shaper/src/test_child.rs`, the R14
parent/child form; the parent asserts that the child ran exactly one test and passed). After the
fix: 0 of 50 of the same sequential runs failed; see the task T-F2 report for the full-suite count.

**state**: CLOSED (task T-F2).

## N160 — `systemd.notify.send`'s address setup, outside S112-F8's anchors: a 108-byte crash and a broken abstract-socket case

- **id**: N160. Not in the inventory. Found by T-G (tail wave), 2026-09-23, while working LEDGER S112-F8,
  whose cited anchors are `systemd.zig:37-53, 136-149`; these sites are at `:100-117`.
- **severity**: low. **kind**: crash (UD202) and functional (UD203), both reference defects.
- **what**: (1) the "too long" check is `path.len < socket_path.len`, so a 108-byte `NOTIFY_SOCKET` passes
  and its NUL is written at `path[108]`; the port had the same index and panicked (UD202). (2) an
  `@`-prefixed abstract address, which the check's own warning names as supported, is copied with its
  `@` and connected with the whole-struct length, i.e. as the pathname `@...` (UD203).
- **fix**: `crates/rustty-os/src/systemd.rs` `notify_address` (both R13 PORT NOTEs), tests
  `notify_socket_as_long_as_sun_path_is_too_long` and `notify_address_lengths`, negative controls in
  UD202/UD203. The Linux-only `send` arm that calls it is not compiled on macOS.
- **state**: CLOSED (task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26): the Linux `send` arm compiles and both tests pass; end to end, `READY=1` reaches an abstract `@name` socket and a path socket gets `RELOADING=1`; the reference's `<` restored panics on a 108-byte value, and its abstract handling delivers nothing (UD202/UD203, LEDGER S112-F8 closed).

## N170 — `randomTmpPath` renders the temp dir through `String::from_utf8_lossy`

- **id**: N170. Not in the inventory. Found by task T-H while closing S111-F12 (the same conversion in
  `allocTmpDir`); outside that row's cited anchors (`file.zig:85-86`).
- **severity**: low. **kind**: behavioral-difference (byte-slice → lossy `String`).
- **oracle**: `src/os/file.zig:121-132` `randomTmpPath` returns `[]u8` from
  `allocPrint("{s}{c}{s}{s}", .{ global.tmpDirPath(), sep, prefix, basename })`: the temp dir's bytes
  verbatim.
- **rustty**: `crates/rustty-os/src/file.rs` `randomTmpPath` returns `String` and formats
  `String::from_utf8_lossy(rustty_global::tmp_dir_path())`, so a non-UTF-8 `TMPDIR` yields a path naming
  a different directory. Its one caller is `crates/rustty-cli/src/ssh.rs` (the ssh control path).
- **proposed fix**: return `Vec<u8>` and pass it to the ssh `Command` as an `OsStr` (touches
  `rustty-cli`, area `src/cli`).
- **state**: **CLOSED** (G-1, 2026-09-28). `randomTmpPath(prefix: &[u8]) -> Vec<u8>` (`crates/rustty-os/src/file.rs`) appends the temp dir's bytes verbatim, as `allocPrint("{s}{c}{s}{s}", ..)` does; the body is `random_tmp_path_in(tmp_dir, prefix)`. The ssh caller (`crates/rustty-cli/src/ssh.rs`) builds `ControlPath=<bytes>` as `Vec<u8>` and passes it through the existing `as_os_str` argv. Test `file::tests::random_tmp_path_keeps_a_non_utf8_temp_dir_verbatim` (dir `/tmp/a\xffb`); `random_tmp_path_is_cached_and_fresh` compares bytes now. Negative control: the helper going through `String::from_utf8_lossy` fails (left has `239, 191, 189` where right has `255`). Was: **OPEN** (triage T-1, 2026-09-28). Unchanged: `randomTmpPath` still returns `String` via `String::from_utf8_lossy` (`crates/rustty-os/src/file.rs:260-270`). Left: return `Vec<u8>`/`OsString` and pass it to the ssh `Command` as an `OsStr`, with a non-UTF-8 `TMPDIR` test. Effort S, no long run; observable only with a non-UTF-8 `TMPDIR` (the ssh control path names a different directory), R1. Was: OPEN.

## N171 — `flatpakExit` can now receive the timer error its `catch unreachable` assumes away

- **id**: N171. Not in the inventory. Found by task T-H while closing S111-F3.
- **severity**: low. **kind**: R13 candidate (a reference `catch unreachable` on an externally
  produced error), for the `src/termio` owner.
- **oracle**: `src/termio/Exec.zig` `flatpakExit`: `const exit_code = r catch unreachable;` over
  `FlatpakHostCommand.WaitError!u8`, and `WaitError = xev.Timer.RunError || Error` (`os/flatpak.zig:115`;
  CORRECTION by task T-H2: this said `:110`).
  The completion's result is `Error.FlatpakMustBeStarted`/`FlatpakSpawnFail` when `waitXev` runs before
  a successful start, and the timer's own `RunError` (`Canceled`, `Unexpected`) when the loop fails.
- **rustty**: `crates/rustty-termio/src/exec.rs` `flatpakExit` is `r.unwrap()`. Since S111-F3 the
  `WaitError::Timer(..)` variant reaches it verbatim, as in the reference; before, the port turned it into
  `Flatpak(FlatpakSpawnFail)`, which the `unwrap` also panicked on. Reachability not measured (flatpak
  only, Linux).
- **proposed fix**: decide under R13 (the `src/termio` batch) whether a failed wait should be handled
  like a process exit, not a panic.
- **resolution (task T-H2, controller ruling on review-T-H)**: not reachable, so an R7 invariant, not
  R13. The `Error` half is set only in states `.init`/`.err` (`flatpak.zig:198-199`); `Exec.threadEnter`
  calls `waitXev` only after `spawn` succeeded, `spawn` waits for the state to leave `.init` and fails
  on `.err` (`:143-150`), so the state is `.started` or `.exited`. The timer half needs a cancel of the
  wait completion (nothing issues one) or an io_uring timeout result other than ETIME/ECANCELED (the
  kernel produces none; kqueue and epoll timers never return `Unexpected`). By reading, per the review
  (the pinned libxev `watcher/timer.zig`, `io_uring.zig:757-761`). The port keeps the checked unwrap as
  `.expect(..)` with a message naming the invariant, and a PORT NOTE at the site
  (`crates/rustty-termio/src/exec.rs` `flatpakExit`).
- **state**: CLOSED.

## N172 — the port's `TempDir` opens the current directory for an empty temp-dir path

- **id**: N172. Not in the inventory. Found by task T-H2 while fixing UD211 (S111-F1, `TMPDIR=/`).
- **severity**: low. **kind**: behavioral-difference (area `src/os`).
- **oracle**: `src/os/TempDir.zig:32-34` `cwd.openDir(global.io(), tmp_dir, .{})`. With `tmp_dir = ""`
  (a present-but-empty `TMPDIR`, which `allocTmpDir` returns verbatim) this fails with `FileNotFound`,
  MEASURED with zig 0.16.0 on macOS (`Dir.cwd().openDir(io, "", .{})` → `error.FileNotFound`).
- **rustty**: `crates/rustty-os/src/tempdir.rs` `Dir::openDirPath` was `open_dir_abs(&self.path.join(sub_path))`,
  and `Path::join("")` is the current directory itself, so `TempDir::init` succeeded and created its
  directory under the current working directory (MEASURED by the T-H2 review: `openDirPath("")` returned
  `Ok("./")`). Reachable in production: `Surface.writeScreenFile` (`Surface.zig:5699`, the
  `write_screen_file` actions) starts with `TempDir.init()`, so with `TMPDIR=` Ghostty fails the action
  and the port wrote the scrollback under the cwd.
- **fix** (R1, task T-H3): `Dir::openDir` and `Dir::openDirPath` fail with `NotFound` for an empty
  `sub_path` (`reject_empty_sub_path`, PORT NOTE on it). Re-MEASURED in task T-H3 (zig 0.16.0, macOS, the
  exact `trimEnd` + `std.Io.Dir.cwd().openDir(io, t, .{})`): `""`, `/` and `//` give
  `error.FileNotFound`; `/tmp/` and `.` open. Tests `tempdir::tests::open_dir_fails_with_not_found_on_an_empty_sub_path`
  and `tempdir::tests::init_fails_with_not_found_when_tmpdir_is_empty` (a child with `TMPDIR=` and a fresh
  cwd; asserts the child ran one test, `TempDir::init` failed with `NotFound`, and the cwd stayed empty).
  Negative control: the check disabled fails both, the second with "TempDir::init succeeded with an
  empty TMPDIR (created .. under the cwd)".
- **state**: CLOSED (task T-H3).

## N173 — after S43-F3, `codepoint-width --mode=wcwidth` is no longer comparable to the pinned perf oracle

- **id**: N173. Found by task T-H2 while measuring S43-F3 (controller Decision B).
- **severity**: low. **kind**: perf-measurement (no behaviour defect).
- **what**: `rustty-bench`'s `cli::main` now runs the global init (`init_with_action(false)`, the
  reference's `global.init(.{ .tool = .. })`), whose `ensureLocale` sets the process locale from the
  environment (en_US.UTF-8 on this host). libc `wcwidth` is much slower under a UTF-8 locale.
  MEASURED (paired, N=9, load ~12, provisional): 238 ms before, 749 ms after; with `LANG=C LC_ALL=C` both
  are 236 ms. The pinned perf oracle (`4749c4e93`, `ws10_suite.py`) predates the `.tool` init and runs
  in the "C" locale, so Fence 1 now reports that config LOST (zig 267 ms, rust 753 ms). The reference at
  `20abdb50a` runs `ensureLocale` in its bench too (`global.zig:208`), so the port matches the current
  reference; the comparison, not the port, is what changed.
- **options for the controller**: record the config in `ws10_suite.py`'s `INCOMPARABLE` with these
  numbers (as `screen-clone.render/small` is), or run the perf lanes with `LC_ALL=C`. Not done here:
  editing a fence is the controller's call (R5).
- **decision** (task S-4, controller ruling: do not add configs to `INCOMPARABLE`): the oracle was
  rebuilt at the pin (`ghostty-bench` ReleaseFast from a `git archive` of the pin, sha256
  `46b5adfb6cd4bfcde6a693a3a6d0cc8c94c6cd825ff66e98c6c6b79d1d95fa2a`,
  `~/rustty-oracle-artifacts/ghostty-bench-20abdb50a`; recipe and reproducibility in
  `.port/tools/perf/README.md`). It runs the reference's global init: its log shows
  `setlocale from env result=en_US.UTF-8`. Both benches now call libc `wcwidth` under the same locale, so
  the config is comparable again. MEASURED (CPU time, interleaved rounds, load 21-25): rustty 0.92x the
  rebuilt oracle at N=25 (810 vs 871 ms) and 0.91x at N=21 (746 vs 814 ms); instructions 5,059M vs
  5,951M. `ws10_suite.py` now uses that oracle (commit `4b60cfac`, sha256-checked, BLOCKED on mismatch),
  and its `INCOMPARABLE` is empty: the `screen-clone.render/small` entry also ended, as its own text
  said it would, because the pin contains 446f80f4e (both benches now run 50,000 render iterations:
  3.785e9 vs 3.874e9 instructions). A provisional ws10 run (load 14.5) held Fence 1 at 14/14, wcwidth
  -8.3%.
- **state**: CLOSED (task S-4).

## N180 — `+page-compression --mode=report` printed to stdout (the S62-F3/F4/F5 class, unlisted)

- **id**: N180. Not in the inventory. Found by T-I (tail wave), 2026-09-23, while working LEDGER
  S62-F3/F4/F5: a grep for `std.debug.print` in `src/benchmark/` at the pin finds nine sites; the
  audit listed seven. The other two are `PageCompression.zig:436` (per-page line) and `:442` (total).
- **severity**: medium (same as S62-F3). **kind**: behavioral-difference (stdout instead of stderr).
- **what**: `crates/rustty-bench/src/page_compression.rs` ported both as `println!`.
- **fix**: all nine sites now use the crate's `debug_print!` macro (`crates/rustty-bench/src/lib.rs`):
  stderr, under the stderr lock, write errors dropped as `std.debug.print`'s `catch return` does
  (`eprintln!` would panic on a failed write). Test
  `crates/rustty-bench/tests/report_output_goes_to_stderr.rs::page_compression_report_goes_to_stderr`
  runs the binary and asserts the lines are on stderr and stdout is empty; negative control
  (`print!` restored in `page_compression.rs` only) fails with "no stderr line starts with
  \"page-compression page=0 \"".
- **state**: CLOSED (T-I).

## N181 — the S115-F1 `end_frame` wrapper in the Linux renderer twin has never been compiled

- **id**: N181. Not in the inventory. Recorded by T-I2 (tail wave), 2026-09-23, from review T-I F3.
  Outside LEDGER S115-F1's cited anchors, which name only the macOS `generic.rs`.
- **severity**: low. **kind**: verification gap (unverified port change).
- **what**: `crates/rustty-renderer/src/generic/linux_impl.rs:3673-3697` (the wrapper) got the same shape as the macOS
  fix: `update_frame` runs `update_frame_impl` and then `font_shaper.end_frame()` on every exit
  (`generic.zig:1271-1274`). HarfBuzz's `end_frame` is a no-op, so the behaviour cannot change, but the
  edit was written on macOS: `cargo check --target aarch64-unknown-linux-gnu -p rustty-renderer` stops
  at cc-rs (no `aarch64-linux-gnu-g++` here) and rvm is down. The site's PORT NOTE says so.
- **fix**: run `cargo build --workspace --features gtk,x11,wayland` and `cargo test -p rustty-renderer`
  on rvm.
- **state**: CLOSED (task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26): `cargo build --workspace --features gtk,x11,wayland`, clippy `-D warnings` for rustty-renderer (all targets) and `cargo test --workspace` with `RUSTTY_REQUIRE_GL=1` (the live-GL renderer tests executed on Mesa llvmpipe) are green on Linux.

## N182 — `expectApproxEqual`'s `expectEqual` fallback arms printed nothing in the port

- **id**: N182. Not in the inventory. Found by review T-I (F8), 2026-09-23; outside LEDGER S80-F2's
  anchors (`comparison.zig:113,128,137-139,192-199`).
- **severity**: low. **kind**: missing diagnostic (test-support text).
- **what**: the reference's `.optional`, tagged-`.@"union"` and `else` arms call
  `testing.expectEqual`, which prints `expected {any}, found {any}` on a mismatch. The port's `i32`,
  `u32` and `Option` impls (and the test-local union) returned the error silently. MEASURED with the
  pinned `comparison.zig` under zig 0.16.0: `expected 1, found 2`, `expected 7, found 9`,
  `expected null, found 1.5`, `expected 3, found null`, `expected .a, found .b`.
- **fix**: `crates/rustty-datastruct/src/comparison.rs` `expect_equal_mismatch` prints that line through
  `print`, used by the `i32`/`u32`/`Option` impls and the test union; `FmtAny` gains `Option` (`null`).
  Test `comparison::tests::expectApproxEqual_expect_equal_fallbacks_print_std_diagnostics` asserts the
  five measured lines; negative control (`expect_equal_mismatch` not printing) fails on the first,
  `["expected 1, found 2\n"]` vs `[]`.
- **state**: CLOSED (T-I2).

## N190 — with port-only emissions subtracted, 11 more reference files showed a deficit their surplus had hidden

- **id**: N190. Not in the inventory.
- **found by**: task tail-s1, the whole-tree sweep that followed N81's fix.
- **severity**: low (gate honesty; no port behaviour changed). **kind**: gate-gap.
- **gate**: `.port/tools/gates/check_log_reconcile.py` (G-LOGFILE), `.port/parity-closure/LOG-PORTMAP.tsv`,
  `.port/parity-closure/LOG-PORT-ONLY.tsv`.

**Method.** Every mapped port emission was paired one to one with a live reference site of the file(s)
sharing its arm: exact normalised message first, then best similarity >= 0.6. Each port emission left
unpaired was then read at the pin on both sides. The confirmed port-only lines and extra static copies
are the 48 rows of `LOG-PORT-ONLY.tsv`. The pairing is a heuristic: a paraphrased counterpart can be
left unpaired, so only rows verified by reading are declared, and a surplus that could not be verified
(below) is left counted.

**Files that went RED once the rows were subtracted** — each had a real missing site masked by a
port-only line or an extra copy. The allowance now equals the measured deficit and its `reason` names
the missing sites (`LOG-PORTMAP.tsv`):

| Reference file | Before (ref/port, allowance) | After | Missing sites and disposition |
|---|---|---|---|
| `src/apprt/embedded.zig` | 54/55, 0 | 54/52, 2 | 7 unrestored with PORT NOTEs (:353 :540 :559 :828 :883 :899 :1983); the arm still carries `main_c.zig`'s 5 emissions (declared overlap, see below) |
| `src/apprt/gtk/class/global_shortcuts.zig` | 14/12, 2 | 14/9, 5 | :239 :345 :365 :567 :580, infallible catch arms (PORT NOTE at `fn refresh`) |
| `src/apprt/gtk/class/surface.zig` | 63/62, 1 | 63/57, 6 (corrected by tail-s1b; tail-s1 read 58, 5) | `set_clipboard`'s `failed to set clipboard content` was a fifth, undeclared port-only line (review tail-s1 B1). Missing: three of the four `unable to remove ...` sites (:1065 :1922 :1929 :1936 fold into `remove_source_id`, counted once), :2735 and :2762 (OutOfMemory arms) and :3618 (`bufPrintZ`). :4307 is NOT missing: it pairs with `clipboard_confirmation_confirm` (surface.rs:2526) |
| `src/apprt/gtk/portal/OpenURI.zig` | 19/13, 6 | 19/12, 7 | :225 :369 :373 :393 :454 :512 :532 (the row's reason already named 7) |
| `src/apprt/gtk/winproto/wayland/Globals.zig` | 6/10, 0 | 6/4, 2 | :157 :239, infallible in wayland-rs / owned String (PORT NOTEs) |
| `src/os/systemd.zig` | 10/8, 2 | 10/7, 3 | :34 :188 `bufPrint` arms; :51 is a real gap, N192 |
| `src/renderer/generic.zig` | 33/26, 7 | 33/25, 8 | 6 DisplayLink-gated + 3 infallible on the Linux arm (9), minus the `opengl/Frame.zig` overlap credit |
| `src/terminal/c/render.zig` | 5/6, 0 | 5/3, 2 | :376 and :1029 are LIVE, in `rustty-vt-c/src/lib.rs`'s `extern "C"` wrappers, which no portmap arm covers |
| `src/terminal/stream.zig` | 128/138, 0 | 128/127, 1 | :1291 `error handling action manually`: the port's `vt_raw` returns `bool` and cannot fail |
| `src/terminal/tmux/viewer.zig` | 19/19, 0 | 19/18, 1 | :1497, inside the reference's TEST helper `testViewer`; the port's is `#[cfg(test)]` (PORT NOTE, S209-F2) |
| `src/termio/Exec.zig` | 58/57, 1 | 58/54, 4 | :731 :748 :840 :1078, each with a PORT NOTE |

None of these is a simple missing log port that could be restored here: all but two are catch arms of
calls that cannot fail in the port, and those two are :51 (Linux-only, N192) and the `c/render.zig`
guards (live, but outside the arm).

**Fixed here.** `src/main_ghostty.zig` was mapped to `crates/rustty-core/src/logging.rs`, whose two
`log_with` calls are logFn's own transport, not reference `log.*` sites; the arm read 2/5 with
allowance 3 and a reason ("sentry/crash arms") that did not describe the five sites. All five (:62
:63 :64 :69 :71) are live in the binary's `main` in `crates/rustty/src/main_rustty.rs` through its
`log_fn` facade. The arm is remapped there, the two Rust-only `run_gtk` error lines (the reference
`try`s out of `main`) are declared port-only, and the row reads 5/5, allowance 0. The LEDGER row
S44-F4 (WAIVED) still carries the old reasoning; it belongs to another area and is not edited here.

**Not handled by LOG-PORT-ONLY.tsv, still counted** (each is a real counterpart of ANOTHER reference
file, so declaring it port-only would be false):
- **Shared-file overlap**: `rustty-apprt-c/src/lib.rs` carries `main_c.zig`'s 5 emissions into
  `embedded.zig`'s arm (+5); `generic/linux_impl.rs` carries `opengl/Frame.zig:65` into `generic.zig`'s
  Linux arm (+1), and `generic.rs:5926` is `metal/Frame.zig`'s `Failed to present render target` in the
  macOS arm. The docstring's "counts, does not attribute" limit.
- **A call site of another file's log helper**: `application.rs` calls `gtk_version::log_version()`
  and `adw_version::log_version()` (the counterparts of `gtk_version.zig:27` and `adw_version.zig:28`),
  so `application.zig` is credited +2. It reads 111/96 with allowance 15, while the consolidated PORT
  NOTE in `Application::new` names 14 sites: up to 3 of its missing sites are unaccounted for.
  UNVERIFIED site by site.
- **`rustty-vt-c/src/lib.rs` is in no arm**: its `extern "C"` option guards (`render_state_set`,
  `render_state_row_set`, `selection_gesture_event_set`, `terminal_set`) are live but uncounted, which
  is part of why `c/render.zig`, `c/selection_gesture.zig` and `c/terminal.zig` carry allowances.
- **Unverified surplus left counted**: `src/terminal/tmux/control.zig` reads 19/20 (the extra is the
  `parse_capture_usize` helper's `failed to match notification`, which may be a fold of several
  reference sites); `pkg/macos/video/display_link.zig` reads 0/1 (nothing to mask).

Update (task tail-s1b). Every item above is resolved, and every file in the table is at allowance 0:
- **Shared-file overlap** is no longer counted twice. `LOG-ATTRIBUTION.tsv` (new, capped, self-tested)
  requires a `default-owner` for any port file in two or more reference files' arms, and `owner` rows move
  single emissions, plausibility-checked (a literal must match the named file's sites at least as well as
  any other holder's, or the call must be a `log_*` emitter defined in the named file's arm). MEASURED
  before: `src/main_c.zig` 27 port against 3 reference sites, `opengl/Frame.zig` 25 against 1; after:
  3/3 and 1/1.
- **The `application.zig` credits** (`log_version` ×2 and a split `error opening gtk-custom-css file`)
  are owned by `gtk_version.zig`/`adw_version.zig` (whose allowances 1 -> 0) and declared `expansion`.
  `application.zig` therefore RISES 15 -> 18: 93 of 111. Its PORT NOTE names 14 sites and a message
  pairing leaves a different 18 unmatched; the site list is not re-adjudicated here (the row's reason
  names both lists, UNVERIFIED) and is the one remaining allowance this batch touched.
- **vt-c `lib.rs`** is in the arms of the seven `c/*.zig` files whose `extern "C"` guards it holds, each
  emission owned by its file: `c/render.zig`, `c/build_info.zig`, `c/key_event.zig`, `c/mouse_event.zig`,
  `c/selection.zig`, `c/selection_gesture.zig` go to 0; `c/terminal.zig` 4 -> 0 (its three
  `stackFallback` OOM arms are declared absent).
- **tmux/control.zig** (review N2): `parse_capture_usize`'s reuse of `failed to match notification` is the
  UD7 R13 handling and is declared port-only (`r13-handling`); 19/19.
- **display_link.zig** (review N1): cvdisplaylink.rs's one emission is generic.zig:991, now in generic.zig's
  macOS arm and owned there; display_link.zig reads 0/0.
- **Test-only reference sites** (review N3): the gate now drops reference sites the reference compiles
  only into its tests, the mirror of the port side's `#[cfg(test)]` rule; `viewer.zig:1497` and 12
  others in 6 more files (whose allowances go to 0).
- **The infallible catch arms** are declared per site and per arm in `LOG-ABSENT-SITES.tsv` (new, capped,
  RED if the reference line is not a live production site, the named port fn does not exist, or the arm
  still emits the message), and four sites the reference itself cannot reach go to `LOG-DEAD-SITES.tsv`
  (`dead-branch`).
- **The real gaps are ported**: embedded.zig:540, systemd.zig:51 (N192), OpenURI.zig:512 (N204).

Allowances over the whole portmap: 201 over 79 files -> 137 over 53. The remaining 52 files other than
`application.zig` were outside review tail-s1's list and were not examined, except where one of the
mechanisms above settled them.

**state**: CLOSED by task tail-s1b.

## N191 — the GTK surface logs a `winproto failed to add subprocess env` warning the reference does not have, and continues where the reference fails

- **id**: N191. Not in the inventory.
- **found by**: task tail-s1 (N190 sweep).
- **severity**: low. **kind**: extra-behavior (inverse defect).
- **oracle**: `src/apprt/gtk/class/surface.zig:1656-1661` at pin 20abdb50a:
  `try window.winproto().addSubprocessEnv(&env);` — no log, the error propagates out of the env build.
- **rustty**: `crates/rustty-apprt/src/gtk/class/surface.rs` `default_termio_env`: on `Err` it emits
  `log::warn!("winproto failed to add subprocess env err={err:?}")` and carries on building the env.

The preserved comment above the port's line quotes a call that exists nowhere in the reference at the
pin (it is already declared a stale comment in `LOG-TEXT-DIVERGENCES.tsv`, whose reason says "Nothing
emits it" — that is wrong: `surface.rs` emits it). So the port both says something the reference does
not and swallows an error the reference propagates. Not fixed here (GTK, another area; the fix changes
the function's error contract). The emission is declared `open-finding` in `LOG-PORT-ONLY.tsv`, so it is
not counted as a reference site; delete that row when this is fixed.

Update (task tail-s1b): decided under R1 — NOT changed, for a verified reason. The reference's
`addSubprocessEnv` can fail only in `x11.zig:331-340` (`bufPrint` of a decimal XID into 64 bytes, which
cannot overflow, and `env.put`'s OutOfMemory); `noop.zig` and `wayland.zig` return nothing. Every port
arm of `add_subprocess_env` (`winproto/x11.rs:581`, `wayland.rs:780`, `noop.rs:171`) returns `Ok(())`
unconditionally. The `Err` arm in `default_termio_env` is therefore unreachable in the port, just as the
reference's propagation is unreachable in practice, and logging-and-continuing there differs from
propagating on no reachable path. Mirroring the `try` would make `default_termio_env` fallible and change
its callers' contract for no observable behaviour. The `LOG-PORT-ONLY.tsv` row is reclassified from
`open-finding` to `rust-only-path` with this reason.

**state**: CLOSED (task tail-s1b; no behaviour change, reason verified at both sides).

## N192 — `launchedBySystemd` folds the open and the read of `/proc/<ppid>/comm` into one call, so `systemd.zig:51` can never be logged

- **id**: N192. Not in the inventory.
- **found by**: task tail-s1 (N190 sweep; `systemd.zig` went from 8/10 to 7/10 once the Rust-only
  MONOTONIC-clock line was subtracted).
- **severity**: low. **kind**: missing-branch (R1).
- **oracle**: `src/os/systemd.zig:37-52`: `openFileAbsolute` fails -> `log.err("unable to open '{s}' for
  reading")` (:38); `readAll` fails -> `log.err("problems reading from '{s}'")` (:51); each breaks false.
- **rustty**: `crates/rustty-os/src/systemd.rs` `launchedBySystemd`: one `std::fs::read(&comm_path)`,
  whose every failure logs :38's text.

A read failure after a successful open is reported as an open failure, and :51 has no counterpart. The
fix is small (open with `File::open`, read at most `TASK_COMM_LEN` bytes with `read`, log each arm with
its own reference text), but the code is `#[cfg(target_os = "linux")]` and this macOS worktree cannot
compile it (`cargo check --target aarch64-unknown-linux-gnu` needs a cross C compiler for rustty-os's
build script), so it is recorded rather than changed blind. `systemd.zig`'s allowance is 3 and names it.

Update (task tail-s1b): FIXED. `launchedBySystemd` opens the file and `readAll`s it into the
`TASK_COMM_LEN` buffer in two steps (`read_comm`, which returns which step failed), and logs :38's and
:51's texts from their own arms. Tests (`read_comm_tests`, Linux): `/proc/self/comm` reads; a missing
file fails at the open; a directory opens and fails at the read (EISDIR). G-LOGFILE: systemd.zig 8/8
(:34 and :188 are `bufPrint` arms the reference cannot reach, now `LOG-DEAD-SITES.tsv`), allowance 0;
deleting the :51 line turns it RED (`reference=8 port=7`). Linux verification: see the task report.

Update (task S-1c, the merge of parity-closure `a3e92e1d`): parity-closure's T-G (LEDGER S112-F8) had
fixed the same gap independently, as `comm_is_systemd` (open, then `readAll` into the `TASK_COMM_LEN`
buffer, two failure arms) with a test that runs on every host
(`comm_check_reads_16_bytes_and_has_two_failure_arms`, both arms driven). The merge kept that version
and dropped this batch's `read_comm` and its Linux-only `read_comm_tests`; nothing of it is left to
verify on Linux. G-LOGFILE `systemd.zig` still reads 8/8, allowance 0.

**state**: CLOSED by task tail-s1b; its code superseded by S112-F8's at the S-1c merge.

## N196 — tests passed without testing anything when their environment was missing

- **id**: N196. Not in the inventory. Found by the tail-s3 task (silent-skip audit), 2026-09-23.
- **severity**: medium (test evidence, not product behaviour). **kind**: gate / test-honesty.
- **what**: a Rust test that returns early reports `ok`. 185 sites across the workspace did exactly
  that when something they needed was missing, measured by the new G-SKIP lint run against the
  pre-fix tree (145 early returns, 22 bodies that are all `#[cfg]`-gated, 17 unpivoted "skipping"
  lines in test helpers, 1 test whose every assertion sat in one else-less `if`). The resource-
  dependent ones: `crates/rustty-apprt-c/tests/macos_window_host.rs` (every hosted-surface KAT,
  `let Some(host) = HostedSurface::spawn() else { return; }`, not covered by `RUSTTY_REQUIRE_METAL`),
  `crates-sys/dcimgui-sys/tests/metal_offscreen.rs`, the CoreText/HarfBuzz shaping tests (system
  fonts), the renderer display-link test, the GTK `gtk_perform_action` KATs (`#[ignore]`d yet silent
  when run), the two GHOSTTY_ORACLE corpus tests, the LZ4 and UD131 opt-in tests, and a
  `posix_openpt` skip in rustty-vaxis. The rest were ports of `if (comptime …) return
  error.SkipZigTest;` (kitty-graphics, glyph-protocol, oniguruma/tmux, wuffs, target OS) written as
  runtime returns, so a build without the feature reported them passed — MEASURED: in a default
  `cargo test -p rustty-vt`, `dcs::tests::tmux_enter_and_implicit_exit` and both
  `image_load_png_*` Wuffs tests used to pass having run nothing; they now report `ignored`, and
  pass with `--features oniguruma,wuffs`.
- **fix**: PORT_RULES R14 (compile-time scope -> `cfg_attr(.., ignore)`; runtime resource ->
  `require_<x>_or_skip` pivot under `RUSTTY_REQUIRE_<X>=1`, or `#[ignore]` + a loud failure; dead
  skips deleted), applied to every site; G-SKIP (`.port/tools/gates/check_silent_skips.py`, HARD
  with `--self-test`) keeps the class out. New variables: `RUSTTY_REQUIRE_SYSTEM_FONTS` (set on the
  macOS CI leg) and `RUSTTY_REQUIRE_DISPLAY` (not set in CI: runner display UNVERIFIED).
- **state**: CLOSED (tail-s3).

## N197 — CI's distcheck runs the extracted tarball's tests with `RUSTTY_REQUIRE_FUZZ_CORPUS=1` inherited

- **id**: N197. Not in the inventory. Found by the tail-s3 task while deciding whether `.git`-probing
  xtask tests could take a strict variable. UNVERIFIED: distcheck was not run.
- **severity**: medium (a CI lane expected to go red). **kind**: CI wiring.
- **what**: `.github/workflows/ci.yml` exports `RUSTTY_REQUIRE_FUZZ_CORPUS=1` into `$GITHUB_ENV` for
  the whole `test` job, and sets `RUSTTY_DISTCHECK=1` on the Ubuntu leg. `xtask distcheck`
  (`xtask/src/dist.rs:361-371`) runs `cargo test --workspace` inside the extracted source tarball
  with the parent environment inherited (only `CARGO_HOME` is overridden). The tarball omits `.port/`
  (`.gitattributes:103`, `.port export-ignore`), so the five corpus replays in
  `crates/rustty-vt/tests/fuzz_corpus_replay.rs` find no corpus and, with the variable inherited,
  panic instead of skipping. By reading, the Ubuntu leg's G-DIST distcheck arm should therefore fail.
- **proposed fix** (NOT applied; out of this task's scope): have distcheck clear the repo-only strict
  variables for the tarball run (`.env_remove("RUSTTY_REQUIRE_FUZZ_CORPUS")`), since a tarball
  legitimately lacks the corpus. The same reason is why the `.git`-probing xtask tests were
  allow-listed in G-SKIP rather than given a strict variable.
- **state**: CLOSED by task T-B (commit `fab80ad0`, merged via `tail-b2`): the distcheck cargo steps drop `RUSTTY_REQUIRE_FUZZ_CORPUS`, pinned by a test whose negative control (removal line deleted) fails for the stated reason; confirmed by the T-B and T-B2 reviews.

## N203 — the GTK surface warned on source-removal and clipboard failures the reference discards

- **id**: N203. Not in the inventory.
- **found by**: task tail-s1b, while making `remove_source_id` a declared facade.
- **severity**: low. **kind**: extra-behavior (the port says something the reference does not).
- **oracle**: `src/apprt/gtk/class/surface.zig:3116-3121` and `:3133-3136` (the horizontal tab-scroll
  handler): `_ = glib.Source.remove(v);` — result discarded, no log. `:4111`: `_ = clipboard.setContent(all);`.
- **rustty**: `crates/rustty-apprt/src/gtk/class/surface.rs` `scroll_horizontal` routed both removals
  through `remove_source_id`, which warns `unable to remove pending horizontal scroll reset source` (that
  text is `dispose`'s, :1936); `set_clipboard` warned `failed to set clipboard content err=` on gdk4-rs's
  `Err` (review tail-s1 B1 had declared that line `rust-only-path`).

Fixed: `scroll_horizontal` calls a new `discard_source_id` (`g_source_remove`, result ignored), and
`set_clipboard` discards `set_content`'s result. The two `open-finding` rows and the B1 `rust-only-path`
row are deleted from `LOG-PORT-ONLY.tsv`; with `remove_source_id` declared a facade, its four remaining
calls are exactly surface.zig's four `unable to remove` sites. Verified by build and by the gate
(surface.zig 60/60 after the declared absent and dead sites, allowance 0). No behavioural test: driving
`scroll_horizontal` needs a realized widget and a touchpad-unit scroll event (UNVERIFIED by test).

**state**: CLOSED by task tail-s1b.

## N204 — OpenURI's `stop_cleanup_timer` panicked on a stale timer where the reference warns

- **id**: N204. Not in the inventory.
- **found by**: task tail-s1b, restoring `OpenURI.zig:512`.
- **severity**: low (a panic inside a GTK callback on a path the port's own `cleanup` normally prevents by
  clearing the field). **kind**: behavioral-difference.
- **oracle**: `src/apprt/gtk/portal/OpenURI.zig:507-516`: `if (glib.Source.remove(timer) == 0)
  log.warn("unable to remove cleanup timer source={d}", .{timer});`.
- **rustty**: `crates/rustty-apprt/src/gtk/portal/open_uri.rs` `stop_cleanup_timer` called glib-rs
  `SourceId::remove`, whose body is `result_from_gboolean!(g_source_remove(..)).unwrap()`
  (glib-0.22.8 `src/source.rs:29-36`): a stale id panics. Its PORT NOTE said the call "returns `()` — it
  has no failure report", which was wrong.

Fixed: `g_source_remove` is called directly and FALSE logs the reference's warning. Test
`stopping_a_stale_cleanup_timer_warns_instead_of_panicking` drives a live source (removed, no record)
and a stale one (one `openuri` warn with the reference text). Linux verification: see the task report.

**state**: CLOSED by task tail-s1b.

## N205 — a requested working directory naming a regular file took the wrong diagnostic

- **id**: N205. Not in the inventory.
- **found by**: task tail-s1b, restoring `embedded.zig:540`.
- **severity**: low. **kind**: behavioral-difference (log text).
- **oracle**: `src/apprt/embedded.zig:529-553`: `openDirAbsolute(wd)` (flags `O_RDONLY | O_DIRECTORY |
  O_CLOEXEC`, plus `O_PATH` on Linux, per Zig 0.16 `dirOpenDirPosix`) fails on a regular file with
  `error.NotDir` and logs `error opening requested working directory`; the `dir.stat` failure logs
  `failed to stat requested working directory` (:540).
- **rustty**: `crates/rustty-apprt/src/embedded.rs` used one `std::fs::metadata`, so a regular file
  passed the "open", then logged `requested working directory is not a directory`, and :540 had no arm.

Fixed: `requested_working_directory_is_usable` opens with the reference's flags and `fstat`s the open
descriptor. Tests: a directory (usable, no record), a missing path and a regular file (both fail at the
open; the file's error is `NotADirectory`). Negative control: without `O_DIRECTORY` the regular-file test
fails. The stat-failure arm itself is not driven (an `fstat` of a just-opened descriptor does not fail on
demand): UNVERIFIED by test, present by reading and by G-LOGFILE.

**state**: CLOSED by task tail-s1b.

## N206 — the URL port and the vaxis CSI parameters use Rust's number parser, not Zig's `std.fmt`

- **id**: N206. Not in the inventory. Found by the review of tail-s9 (the Zig number-parser
  consolidation), which measured both with Zig 0.16.0.
- **severity**: low. **kind**: behavioral-difference.
- **oracle**: Zig 0.16.0 `std/Uri.zig` (`parseInt(u16, …, 10)` for the port) as used by Ghostty's OSC 7
  handling; libvaxis 0.6.0 `src/Parser.zig` (`parseUnsigned` / `parseInt` for CSI parameters).
- **rustty**: `crates/rustty-os/src/uri.rs` `parse_int_u16` / `parse_int_u8_hex` and
  `crates/rustty-vaxis/src/parser.rs` `parse_unsigned` use Rust's `str::parse` / `from_str_radix`, which
  reject the `_` separators, the leading `+` and the `-0` that Zig accepts: `file://host:8_0/x` is port 80
  in the reference and an InvalidPort in the port.
- **fix**: route both through `rustty_zigstd::fmt` (rustty-os already depends on it). Whether `%-0`
  decoding to NUL in `std.Uri` should be reproduced is a separate R13 question.
- **verification** (task tail-s9b, MEASURED): a Zig 0.16.0 probe built against the pinned libvaxis
  0.6.0 (`vaxis-0.6.0-BWNV_CrbCQCscGpzsAlR402rYQ_tV3aAl081c2iRRkka`) and `std.Uri`, and a Rust
  harness over the port's public API, ran the same 1940 inputs (every 1-3 byte string over
  `0 1 5 9 a f F _ + - space 0xFF`, plus overflow edges, `8_0`, `+80`, `-0`, prefixes) through 19
  shapes: `parseInt(u16, .., 10)` and `parseInt(u8, .., 16)`, the port of `file://host:<s>/x`, the
  decode of `/a%<s>b`, and the vaxis parser on OSC 4 / OSC 10-12 numbers, an OSC 4 channel, the
  XTWINOPS size, kitty keys (codepoint, modifiers, alternates), CSI `~`, CSI modifiers, DECRPM, DSR
  and SGR mouse (36,860 results, plus 31 exact sequences, 38,831 in all).
  - **URL half: refuted as stated.** At the base (ff351484) `uri.rs` no longer used `str::parse`:
    task T-G (37d69654, S112-F9) had already given it a hand-written copy of Zig's `parseInt`. The
    base port matched Zig on all 3880 URL results (0 mismatches). What remained was a fourth copy of
    the parser, now deleted: `parse_int_u16` / `parse_int_u8_hex` call `rustty_zigstd::fmt::parse_int`,
    and its `parseInt` table test is dropped as the twin of `rustty_zigstd::fmt::tests::test_parse_int`.
  - **vaxis half: confirmed.** 263 mismatches at the base: `_` refused in the OSC numbers
    (`ESC ] 4 ; 1_0 ; ..` is index 10 in the reference); `+N` / `-0` refused in every `parseParam`
    CSI number, which libvaxis 0.6.0 reads with `parseInt` (0.5.1, which the crate otherwise follows,
    used `parseUnsigned`). In a CSI a `_` is a final byte, so `_` reaches only the OSC numbers, and the
    `parseUnsigned` CSI sites refuse a sign in both. Fixed: `parser.rs` `parse_unsigned` /
    `parse_param` and `cell.rs` `parse_unsigned_u8` call `rustty_zigstd::fmt`
    (`parse_unsigned_type` / `parse_int_type` / `parse_unsigned::<u8>`), and rustty-vaxis gains a
    `rustty-zigstd` edge (`check_acyclic.sh`, `cargo_cycle_check.py` GREEN).
  - **After the fix: 152 mismatches, all explained.** 116 are the R13 fix below (every input the
    reference decodes through a sign). 36 are SGR mouse coordinates: libvaxis 0.6.0 reads them as
    `i16` into an `i16` `Mouse.col`/`row` (so `0` is column -1, `-5` column -6, `40000` refused),
    while the port keeps 0.5.1's `u16`. That type change is part of the deferred vaxis bump
    (`.port/phase-3/NON_PORTS.md` §G2, which names `Mouse.zig` `u16 -> i16`), noted at `parse_param`.
    No other result differs.
- **R13 decisions**:
  - `%-0` / `%+X` in `std.Uri` percent decoding: **fixed (UD230)**. RFC 3986 escapes are two hex
    digits, and the decoder's own `catch continue` keeps every other malformed escape as written; the
    sign is an accident of reusing `parseInt`, and it puts a NUL or a control byte into an OSC 7 pwd
    the sender never wrote. Listed in `docs/ghostty-bug-fixes.md`. (RFC 3986 also rules out `+80` as
    a port, so the RFC alone does not separate the two cases: the reason is that decoding a sign
    invents bytes and corrupts the data, while a lenient port parse yields the number the sender
    meant.)
  - `_`, `+` and `-0` in the URI port number: **faithful**. It is std's grammar, Ghostty has no port
    grammar of its own, and Ghostty never reads the port except in the MAC-address repair of
    `os/uri.zig` `parse`, whose outcome does not change (`12:34:56:78:90:+9` fails
    `isValidMacAddress` whether or not the port parses).
  - `_`, `+` and `-0` in the vaxis numbers: **faithful**. libvaxis has no parser of its own that
    refuses them, so there is no UD141-style in-reference evidence; the peer is the terminal.
- **tests** (fail first, negative controls one at a time, task tail-s9b): rustty-os
  `percent_escapes_decode_only_two_hex_digits` (replaces `percent_escapes_follow_zig_parse_int`) and
  `osc7_path_keeps_a_signed_escape` (control: hex check disabled, NUL where `%-0` expected);
  `port_numbers_follow_zig_parse_int` (unchanged; control: `str::parse`, `Err(InvalidPort)` for
  `8_0`). rustty-vaxis `osc_numbers_take_zig_digit_separators` (control: `_` refused, `None` where
  index 10 expected), `csi_params_take_zig_parse_int_signs` (control: `parseUnsigned`, `None` for
  `ESC [ +97 u`), `csi_unsigned_numbers_refuse_a_sign` and `rgb_from_spec_channels_are_zig_parse_unsigned`
  (both passed at the base; controls: `parseInt` at those sites, the sign accepted).
- **state**: CLOSED (task tail-s9b).

## N207 — `+terminal-stream` on an OSC 8-heavy corpus is about 3.3x slower than the oracle

- **id**: N207. Found by task tail-s2 (S-2b) while measuring the N82 fix. Not in the inventory.
- **severity**: medium (performance). **kind**: perf gap; not a fidelity defect.
- **corpus**: 16 MiB (16777216 B), generated once to a file and truncated (never piped into the
  bench), like the ws10 corpora. The generator (`gen_osc8.py`, task tail-s2 scratch, reproduced in the
  task tail-s2 report) follows the reference generator's `hyperlink_start` / `hyperlink_end` grammar
  (`src/synthetic/Osc.zig`) in the `--style=streaming` framing: `ESC ] 8;` + `id=<alnum 0-16>` half
  the time + `;https://localhost` + 2-5 `/<alnum 0-16>` segments + `ESC \`, then 1-8 printable
  alphanumerics, then `ESC ] 8;; ESC \`, and `\r\n` after 10% of links. Python `random.Random(82)`.
- **method**: `ws10_suite.measure` (one warmup, then n timed runs, stdout/stderr discarded), `+terminal-stream
  --data=osc8.bin`, the pinned oracle `.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench`
  against `target/release-fast/rustty-bench`, alternating which goes first each round.
- **measured** (PROVISIONAL: 1-minute load 17-25 on every run; other agents were building): oracle median
  110-132 ms; rustty 368-444 ms, i.e. +215% to +303% over five runs (4 rounds x n=9, or 2 x n=5), both
  before and after N82's fix. The hash is not the cause (N82's change moved rustty by -17% to +7%).
  The page grows its hyperlink and string capacity repeatedly on this corpus (`info(page_list):
  adjusting page capacity` lines on stderr); whether that path, the logging, or something else is the
  cost is UNVERIFIED.
- **not done**: no fix attempted; the perf batch owns it. Re-measure at load below `IDLE_MAX` first.
- **resolution** (task S-4, MEASURED; measured on the merge of `tail-s2`, so with N82's Wyhash hash):
  the gap is the retired oracle, not the port. Against Ghostty's bench built at the pin (N173's
  rebuilt oracle), on the same `osc8.bin` (the file itself, copied from the tail-s2
  scratch): PROVISIONAL (load 17-19, above `IDLE_MAX` 2.0) wall, interleaved N=15: pin oracle 357.1 ms,
  rustty 356.8 ms (paired ratio 1.002), retired `4749c4e93` oracle 109.4 ms; CPU time: pin oracle 357.7,
  rustty 358.3 ms (paired 0.992). Deterministic: instructions retired rustty 5,029M vs pin oracle 5,240M
  (0.965). Both log the same diagnostics: 126,363
  `start_hyperlink err=OutOfSpace` warnings and 1,004 `adjusting page capacity` lines each (the page
  reaches the 65,535-byte hyperlink ceiling and the reference drops further links there too), so both
  do the same work; Ghostty itself became about 3.3x slower on this corpus between `4749c4e93` and the
  pin. Why (measured by review S-4): the retired binary writes 0 diagnostic lines on this corpus (its
  bench never ran `global.init`, so logging was never enabled), the pin's writes 127,375, because
  `benchmark/cli.zig:55` now runs `global.init(.{ .tool = ... })`, which turns on the stderr and macOS
  unified-log sinks, and `main_ghostty.zig`'s `logFn` creates an `os_log` logger per message. Controller
  ruling: the warning per dropped hyperlink is the reference's intended behaviour (a diagnostic at a
  designed capacity ceiling), not an R13 defect, and the port does not diverge. An xctrace profile of rustty puts 32% of samples in writing those warnings and 29% in
  `increase_capacity` (the reference was not profiled). Re-measured at the end of task S-4
  (PROVISIONAL, N=21, CPU, load 21-25): rustty 356.3 ms, pin oracle 365.4 ms (paired 0.98).
- **state**: CLOSED (task S-4; not a port gap).

## N208 — `Offset(T).Slice` field order differs from the reference on 64-bit targets

- **id**: N208. Found by task tail-s2 (S-2b) while probing the hyperlink tag byte. Not in the inventory.
- **severity**: low. **kind**: R1 fidelity candidate (page-mapping bytes; no behaviour shown to differ).
- **oracle**: `src/terminal/size.zig` `Offset(T).Slice = struct { offset: Offset(T), len: usize }` is a
  Zig auto-layout struct. MEASURED with Zig 0.16.0 at `20abdb50a`: on aarch64-macos it is 16 bytes with
  `len` at 0 and `offset` at 8 (Zig puts the 8-aligned field first); on wasm32-wasi it is 8 bytes with
  `offset` at 0 and `len` at 4.
- **rustty**: `crates/rustty-render-types/src/term_size.rs` `OffsetSlice<T>` is `#[repr(C)]` in the
  declared order: `offset` at 0, `_padding` at 4, `len` at 8 on 64-bit. So every slice stored in a page
  mapping (a hyperlink's `.explicit` id and `uri`, grapheme-map values) has its two words swapped
  relative to the reference on 64-bit. wasm32 matches.
- **interaction**: the hyperlink `.implicit` `u32` sits at bytes 0-4 in both (the reference's union
  starts every field at byte 0), which in the reference overlaps the low bytes of `len`. A fix that
  reorders `OffsetSlice` must keep `PageEntryId::Implicit` writing bytes 0-4 (today it writes them
  through `payload.offset`).
- **not done**: `OffsetSlice` is shared across areas (render-types, grapheme map, hyperlinks); out of
  task tail-s2's scope.
- **review (S-2)**: confirmed with an independent mock: `len` @0 / `offset` @8 on aarch64-macos (Debug,
  ReleaseSafe, ReleaseFast) and x86_64-linux; `offset` @0 / `len` @4 on wasm32-wasi. A fix would also
  have to rework `PageEntryId` and audit every other auto-layout struct in page memory.
- **ruling** (controller, 2026-09-23): WAIVED as not observable. The field order of a non-`extern` Zig
  struct is unspecified by the language and differs per target; the raw page bytes are never persisted
  or compared with the oracle's; size, alignment and stride parity are already enforced (N88, R13-L).
  By contrast the union tag numbering fixed under N82 is language-defined (declaration order), so it
  stays fixed. The PORT NOTE at `crates/rustty-render-types/src/term_size.rs` that claimed "the same 4
  padding bytes" now gives the real positions. `ADJUDICATION.md#n208`.
- **state**: WAIVED (task tail-s2, S-2c).

## N215 — against the oracle rebuilt at the pin, `+terminal-stream` retired 27% more instructions than Ghostty

- **id**: N215. Found by task S-4 while rebuilding the perf oracle at the pin (N173). Not in the
  inventory.
- **severity**: medium (performance; a STRICT Fence 1 config). **kind**: perf gap; not a fidelity defect.
- **what**: on `ascii.bin` (the ws10 `terminal-stream/ascii` config) rustty retired 1,389.6M instructions
  to the pin oracle's 1,097.4M, and was 7-15% slower in CPU time (paired, load 15-40). Two causes,
  MEASURED with xctrace profiles and instruction counts:
  1. the printable-run scans (`Stream::next_slice_capped` in `stream.rs`, the codepoint run scan in
     `print_slice_fill`) built a per-lane bitmask on EVERY group of 8 codepoints. NEON has no movemask,
     so LLVM emitted a scalar extract per lane (about 21 instructions per group against the
     reference's 12). The reference tests the group with `@reduce` alone and builds the bitmask only
     on the group that ends the run. Both scans now do the same, the reduction spelled as an unsigned
     min/max so LLVM emits one vector op and an across-lanes reduce (a boolean-AND spelling still
     lowered to a partly scalar sequence): 1,389.6M -> 1,254.9M -> 1,095.5M (intermediate builds;
     the final candidate, with cause 2 fixed too, measured 1,095.7M, below).
  2. the bench read the corpus through `DataFile`'s 256 KiB `BufReader`, which a 64 KiB request does not
     bypass, so the whole corpus was memcpy'd once (about 3% of samples). The reference's
     `TerminalStream` reads unbuffered (`f.reader(io, &.{})`); `DataFile::unbuffered` is that, and the
     bench's PORT NOTE claiming the read was already unbuffered is corrected.
- **fix**: commit `8c1a3e38`. Tests: `unbuffered_reads_the_file_directly` (control: return the
  `BufReader` -> "read 1 went through the buffer"); the existing lane-boundary test
  `stream_print_slice_batches_printable_runs` and the print-vs-`print_slice` differential fuzz
  (controls: stream stop never detected -> "expected exactly two printable runs"; scan threshold widened
  past 0xFF -> the differential's cursor mismatch).
- **measured**: instructions (deterministic) 1,095.7M for the final candidate (pin oracle 1,097.4M).
  PROVISIONAL (above `IDLE_MAX` 2.0): wall, interleaved N=31, load 13: 59.0 ms before, 51.8 ms after
  (0.879), pin oracle 52.9 ms; CPU N=25, load 21-28: 49.9 ms vs pin oracle 53.7 ms (0.93). Every other
  ws10 config: instructions within 1% of before (`screen-clone.noop` -1.8%). The authoritative idle ws10
  run is still owed (task S-4 report).
- **state**: CLOSED (task S-4).

## N220 — the deferred uucode revision bump (`NON_PORTS.md` §G1): CI still pinned Zig 0.15.2, which cannot build the harness's uucode

- **id**: N220. Not a ledger row: the deferral lived only in `.port/phase-3/NON_PORTS.md` §G1,
  `.port/phase-2/DEFERRALS.md` ("Still open — summary") and `.port/phase-3/DEFERRAL_REAUDIT.md` row 13.
  Discharged by task tail-s10a at the user's instruction to address every parity gap.
- **severity**: medium (CI lane), none (data). **kind**: dependency-revision / CI-toolchain.
- **oracle**: ghostty@20abdb50a `build.zig.zon` (uucode `2826a37a…`, hash `ZZjBPlK5…`),
  `src/build/uucode_config.zig`, `src/build/UnicodeTables.zig`, `src/build/SharedDeps.zig:64-78`,
  `src/unicode/{props_uucode,symbols_uucode,lut,props,grapheme}.zig`.
- **what was found**: the harness (`.port/phase-0/harness/unicode/`) had already been moved to uucode
  2826a37a, the pin's config and the `uucode.types` / `uucode.grapheme` API by `986d90c8`, but
  `.github/workflows/ci.yml` still installed Zig 0.15.2 for the G4-uni job while the harness's
  `build.zig.zon` requires 0.16.0. So on CI G4-uni could not build its oracle: RED without comparing
  anything. §G1 still read DEFERRED.
- **fix**: CI installs Zig 0.16.0 for that job. No other gate in that job depends on 0.15.2: every other
  zig consumer (`check_lz4_byte_exact.sh`, `check_ref_item_geometry.py`, `stb-resize-oracle/run.sh`, …)
  also needs the ghostty oracle checkout, absent on CI, and locally they already run on 0.16.0. The
  harness's stale version text (the BLOCKED message said "install zig 0.15.2") and the provenance
  headers of the three generated tables, `lib.rs` and `props.rs` (`uucode.x.types` ->
  `uucode.types`) now name the revision.
- **table differential** (the recipe's step 3), all against uucode 2826a37a + the pin's config:
  - the pin's own `props-unigen` / `symbols-unigen`, built as `UnicodeTables.zig` builds them and run:
    `props_data.rs` STAGE1/2/3 (8192 / 31744 / 30) and `symbols_data.rs` (8192 / 1792 / 2) are identical
    element for element. Negative controls: one flipped STAGE3 field / one flipped STAGE2 entry in a copy
    each reported exactly 1 difference.
  - G4-uni (`run_unicode_diff.sh`) under Zig 0.16.0: 0 diffs for width, is_symbol, props flags and
    grapheme_break_no_control over 0..=0x10FFFF and the 8192 grapheme precompute entries
    (`uucode.grapheme.computeGraphemeBreakNoControl`).
  - `case_folding_full` (1585 code points, `rustty-unicode` `case_folding_data.rs`) and
    `is_emoji_presentation` (1219, `rustty-font-shaper` `uucode.rs`), dumped from the live uucode over
    the full range: 0 differences each (negative controls: a dropped and an altered entry each
    reported).
  - Codepoints changed, per class: **none**. The UCD inputs are byte-identical between the two
    revisions (`diff -rq ucd/`: only a new `BidiMirroring.txt` and `.gitignore`), and the restructured
    derivations (`x/config_x/wcwidth.zig` -> `components.zig`, `x/grapheme.zig` -> `grapheme.zig`)
    produce the same values. `ascii.zig`, `utf8.zig`, `code_point.zig` are byte-identical; `BreakState`
    and `GraphemeBreakNoControl` keep their order. `checksum.rs` is unchanged because the arrays are.
- **perf**: the `release-fast` `rustty-bench` binary is byte-identical before and after (sha256
  `ea2e5b2b…`; only comments changed). Paired interleaved sweep anyway (N=9, utf8 corpus, `LC_ALL=C`,
  load average ~20–22 on a shared host): codepoint-width.table −1.1%, .simd −0.0%, grapheme-break.table
  −1.8%, is-symbol.table +0.1%, all within noise.
- **not settled here**: `rustty-vaxis`'s vendored uucode 0.1.0 subset (UCD 16.0.0). At the pin
  libvaxis 0.6.0 imports Ghostty's uucode 2826a37a through `external_uucode` (the `libvaxis_only`
  table: `east_asian_width`, `general_category`, `grapheme_break`, plus `runtime`'s
  `is_emoji_presentation`), so moving it belongs to §G2 (vaxis 0.5.1 -> 0.6.0), task S-10b.
- **state**: CLOSED (task tail-s10a).

## N240 — the deferred `vaxis` 0.5.1 -> 0.6.0 and `zf` 0.10.3 -> 0.11.0 bumps (`NON_PORTS.md` §G2), with vaxis's uucode 0.1.0 -> 2826a37a

- **id**: N240. Not a ledger row: the deferral lived in `.port/phase-3/NON_PORTS.md` §G2,
  `.port/phase-2/DEFERRALS.md` ("Still open — summary", and the Chunk 1b uucode entry) and
  `.port/phase-3/DEFERRAL_REAUDIT.md`. Discharged by task tail-s10b at the user's instruction to address
  every parity gap.
- **severity**: medium (behaviour of the CLI TUIs; four reachable crashes found). **kind**:
  dependency-revision.
- **oracle**: ghostty@20abdb50a `build.zig.zon` (vaxis `vaxis-0.6.0-BWNV_CrbCQCsc…`, zf
  `zf-0.11.0-OIRy8X-RAAA…`, uucode `2826a37a` `ZZjBPlK5…`; the three cached tarballs re-hashed with
  `zig fetch` to exactly these), `src/build/SharedDeps.zig:24-78, 512-519` (vaxis built with
  `external_uucode = true` over the singleton uucode module and `src/build/uucode_config.zig`),
  `src/cli/{list_themes,list_colors,list_keybinds,boo}.zig`.
- **what was ported** (per file, 0.5.1 -> 0.6.0; `std.Io` / allocator / `std.ArrayList` churn noted but
  not reproduced, as the port has no `std.Io`):
  - `uucode` (vendored subset in `rustty-vaxis/src/uucode/`): 2826a37a's `grapheme.zig` (20-variant
    `GraphemeBreak` with `emoji_modifier_base` / `emoji_modifier`, the UTS #51 modifier tailoring in
    `computeGraphemeBreak`, the 2000-entry precompute, `nextCodePoint` / `.code_point`,
    `peekCodePoint`, `nextGrapheme` / `peekGrapheme`, `utf8Iterator`), `types.zig`, `utf8.zig` tests;
    `build.rs` follows `components.zig` (`EmojiData`, `GraphemeBreakDerived`, `" \t\r"` trims) over the
    vendored UCD 17.0.0 (the same tarball as G4-uni; `ucd/PROVENANCE.md` re-digested).
  - `gwidth.zig`, `unicode.zig`, `Parser.zig` (the uucode rename; `CSI Z` is shift+tab; SGR mouse
    coordinates are `parseParam(i16, ..)`, closing the 36 mouse results N206 left open), `Mouse.zig`
    (`col`/`row` `i16`), `ctlseqs.zig` (the three multi-cursor sequences), `Screen.zig` (`Cursor`,
    `cursor_secondary`), `InternalScreen.zig` (cursor fields removed; bounds `<=`; its new test),
    `Window.zig`, `Vaxis.zig` (the render writes nothing when nothing changed, `startRender`,
    multi-cursor output and `setTerminalCursorSecondaryColor` / `resetAllTerminalSecondaryCursors` /
    `addTerminalSecondaryCursor`, `translateMouse` over `i16`, `queryTerminal(Duration)`,
    `transmitImage` over a copy of a `*const` image, the env map; its new test), `Key.zig`
    (`matchText` on empty text), `queue.zig` (`tryPush` / `tryPop` under one lock, `popAndSignalLH` /
    `pushAndSignalLH`, `drain` signals `not_full`), `Loop.zig` (`init` is the constructor,
    `installResizeHandler`, `tty.getWinsize()`), `tty.zig` (the SIGWINCH action installed once,
    `getWinsize` methods, Windows AltGr, `0xDF` and `i16` mouse), `widgets/TextInput.zig` (the
    readline word motions, Ctrl+W `deleteWordBeforeWhitespace`, 13 new tests), `vxfw/vxfw.zig` (`Tick`
    on the monotonic clock, `copyToClipboard` / `setTitle` copy), `vxfw/App.zig` (`init(buffer)`,
    frame pacing on the monotonic clock, `resetEventState` around timer ticks, its new test),
    `vxfw/Text.zig`. Unchanged apart from anonymous `refAllDecls` tests: `Cell`, `event`,
    `GraphemeCache`, `Image`, `widgets.zig`, `vxfw/Center.zig`.
  - `zf` (`filter.zig`, `zf.zig`, `clib.zig`): `token` -> `needle`, `rankToken` / `highlightToken` ->
    `rankNeedle` / `highlightNeedle` (Zig and C exports), `RankOptions.to_lower = true` ->
    `case_sensitive = true` (the default FLIPS to case-sensitive), `indexOfInsensitive` lowers the needle
    too, `highlight` sorts and deduplicates for several needles (`deduplicateMatches` and its test).
- **the zf polarity decision**: ghostty@20abdb50a `list_themes.zig:377-386` passes
  `.case_sensitive = false, .plain = true` explicitly (with a NOTE that it replaces `.to_lower = true`);
  `rustty-cli` passes exactly that. Its tokens are lowercased before ranking, so the new needle
  lowering cannot change a result there. MEASURED: 470 names (Ghostty.app's 463 themes + 7 path-like
  strings) x 1521 queries, four columns (ghostty's call, the default options, insensitive raw
  needles, `highlight`): 714,870 rows bit-identical to a Zig 0.16.0 build of zf 0.11.0. Negative
  controls: `case_sensitive: true` at the call site fails `rank_filtered_inclusion` /
  `rank_filtered_ordering`; the default flipped back fails `rank_library_interface` and
  `rank_options_default_is_case_sensitive`; the needle not lowered fails the latter.
- **uucode differential** (vaxis's four fields, the S-10a recipe): a Zig 0.16.0 probe against uucode
  2826a37a with ghostty's `uucode_config.zig`, wired as `SharedDeps.zig` wires it, dumped
  `east_asian_width`, `general_category`, `grapheme_break`, `is_emoji_presentation` for
  0..=0x10FFFF: **0 differences** from `rustty_vaxis::uucode::get` (negative control: one flipped byte,
  1 difference). Against the 0.1.0 / UCD 16.0.0 tables the move changed 157 `east_asian_width`, 4804
  `general_category`, 1556 `grapheme_break` (689 extended_pictographic -> other, 671 other -> InCB
  consonant, 134 extended_pictographic -> emoji_modifier_base, 39 other -> InCB extend, 14 InCB extend
  -> linker, 5 InCB extend -> emoji_modifier, 3 other -> spacing_mark, 1 prepend -> other) and 7
  `is_emoji_presentation` values; each is the oracle's. `GraphemeBreakTest-17.0.0.txt`: 766 cases, all
  pass with the modifier tailoring.
- **gwidth / grapheme differential**: vaxis `gwidth` (`.unicode`, `.wcwidth`, `.no_zwj`) and
  `unicode.graphemeIterator` over 1,334,878 inputs (every code point alone, surrogates as WTF-8, the 766
  test sequences, 200,000 random sequences over emoji / modifier / ZWJ / flag / Hangul / Indic /
  control code points, 20,000 random byte strings): identical to the Zig oracle except 36 inputs on which
  the reference traps and 35 on which its grapheme ranges overlap, all UD260. (A ReleaseSafe build of
  the probe trapped at `utf8.zig:35` on the single-code-point section where the Debug build did not;
  the Debug build was used as the oracle. The S-10b review did NOT reproduce it: its own ReleaseSafe
  probe ran libvaxis 0.6.0 `gwidth` over every code point alone with all three methods, with no trap
  and 0 differences from the port; and the cast at `utf8.zig:35`, `@intCast(utf8d[byte])` into
  `IntFittingRange(0, 11)`, cannot fail because every table entry is at most 11. Treated as an artifact
  of the S-10b probe, not a reference defect.)
- **parser / render differentials**: 4,204 SGR / X10 mouse and `CSI Z` sequences (every 1-3 byte
  string over `0 1 5 9 a f F _ + - SP 0xFF` in each coordinate, `i16` edges, X10 bytes): parsed events
  identical to libvaxis 0.6.0. Eleven render steps (cell, cursor, secondary cursor, hide, reset):
  byte-identical to a libvaxis 0.6.0 built with the UD261 fix; the unpatched reference differs exactly
  at the four UD261 steps. libvaxis 0.6.0's own test suite, built against ghostty's uucode: 151 of 152
  pass (the failure is the cwd-dependent `vxfw` doctest).
- **reference defects found** (R13, all fixed with tests and negative controls): UD260 (gwidth /
  grapheme start on invalid UTF-8; since 0.5.1), UD261 (inverted secondary-cursor comparison), UD262
  (`translateMouse` `@intCast`), UD263 (vxfw `@intCast` of a negative mouse position), UD264
  (TextInput word motions reach `unreachable` in `std.unicode.utf8Decode`), UD265 (`trimEnd` for
  `trimLeft` in `Text` soft wrap), UD266 (`+list-themes` stopped installing the resize handler in the
  Zig 0.16 migration). All in `docs/ghostty-bug-fixes.md`. Recorded faithfully: UD267 (`kind: latent`,
  the queue's edge-only signalling can leave a second blocked producer or consumer waiting; found by
  the S-10b review).
- **review follow-up (task tail-s10c)**: UD261's fix is gated on `caps.multi_cursor` (without it, a
  secondary cursor added on a terminal lacking the capability made every render repaint; test
  `secondary_cursor_without_multi_cursor_does_not_repaint` + negative control); UD263's reachability
  is restated (vxfw turns pixel reporting off, so a coordinate of 0 or below comes from a non-conforming
  terminal or injected bytes, not an ordinary drag); `check_test_counts.py`'s vaxis row now also covers
  `Vaxis.zig` and `vxfw/{vxfw,App,Text,Center}.zig` (zig 105, one allowance for the vxfw doctest source
  scan), with `--self-test` arms binding the row and the per-row verdict (each negative-controlled).
- **not settled here**: libvaxis 0.6.0 also moved zigimg `d7b7ab0b` -> `d695acd9`; the older tarball is
  not in the local cache, so that delta was not diffed. It reaches only vaxis's image tier
  (`transmitImage` / `loadImage`), which no Ghostty code calls. What WAS checked (S-10b review F4): the
  two conversions libvaxis 0.6.0 reaches (`Vaxis.zig:1066-1070`, `convertNoFree` to `.rgb24` /
  `.rgba32`) in `d695acd9` (`PixelFormatConverter.zig:532, 561, 772-778`; `color.zig:19-75, 478-505`:
  `rgbToRgba` copies the colour, `rgbaToRgb` premultiplies alpha in f32 with `@round` and clamp) are the
  formulas `crates/rustty-zigimg/src/convert.rs` implements; `convertNoFree` differs from `convert` only
  in not freeing the old pixels, which is irrelevant to the port's owned `Vec`; and the decode / PNG
  encode paths are re-homed onto Ghostty's codecs by design, so a zigimg delta there cannot reach the
  port. Only the `d7b7ab0b` side of the diff remains unread. The vxfw doctest test (`All widgets have a
  doctest`, a source scan) is not ported (it was not before either).
- **state**: CLOSED (task tail-s10b).

## N250 — the tmux viewer's pane captures ask tmux for `-C` and are decoded (R13 fix, UD12)

- **id**: N250.
- **found by**: task S-12 (controller ruling: UD12 is fixed under R13 as extended 2026-09-23).
- **severity**: low. **kind**: extra-behavior (reference defect fixed, R13).
- **oracle**: `src/terminal/tmux/viewer.zig:1317`, `:1331` (the `capture-pane -p -e` commands), `:1061`,
  `:1102` (the replies written into the pane).
- **rustty**: `crates/rustty-vt/src/tmux/viewer.rs:2209`, `:2232` (`-C` added), `:2438`
  (`unescape_capture`), `:1791`, `:1887` (decode sites).

The port's capture commands differ from the reference's by `-C`, and the port decodes tmux's `\ooo`
and `\\` escapes before writing a capture. Without it, the first ESC of a coloured capture ends the
control-mode DCS and the session. For the grid text the pane receives the same bytes `-e` alone
produces; a hyperlink's URI and id, which tmux 3.4+ stores vis-encoded (`hyperlinks.c:148`, `:154`),
are decoded back to the application's own bytes, where `-e` alone would give the pane the vis form
(read from tmux's source; review F1). Reference
defect: `UPSTREAM-DEFECTS.md` UD12; tests `coloured_pane_captures_keep_the_control_session_and_reach_the_pane`
and `unescape_capture_inverts_tmux_capture_escaping`.

**state**: CLOSED by task S-12.

## N251 — a `list-panes` reply skips a pane whose captures are still queued (R13 fix, UD9)

- **id**: N251.
- **found by**: task S-12 (controller ruling: UD9's remaining history damage is fixed under R13 as
  extended 2026-09-23).
- **severity**: low. **kind**: extra-behavior (reference defect fixed, R13).
- **oracle**: `src/terminal/tmux/viewer.zig:908-1037` (`receivedPaneState` applies every pane in the
  reply), `:680-690` (a new pane's captures, then `pane_state`).
- **rustty**: `crates/rustty-vt/src/tmux/viewer.rs:1519` (`received_pane_state`).

The reference applies a `list-panes` reply to every pane it lists, including a pane added after the
command was queued whose captures have not yet arrived; its scroll region and cursor then garble the
history and visible captures written into it. The port skips such a pane; its own `list-panes`,
queued after its captures, applies the state. Reference defect: `UPSTREAM-DEFECTS.md` UD9 (whose
earlier assert divergence is N43); test `pane_state_for_a_pane_with_captures_queued_waits_for_its_own_reply`.

**state**: CLOSED by task S-12.

## N255 — libxev's FreeBSD loop-wakeup `@panic` stub is unreachable, not "reachable only through an explicit `Loop::wakeup()`"

- **id**: N255. Not a ledger row: the item lived in `.port/phase-2/DEFERRALS.md` ("FreeBSD — the
  platform's explicit ledger row", item 3). Settled by task tail-s13.
- **severity**: low. **kind**: reachability correction (reference defect recorded, not fixed).
- **oracle**: libxev `9ce8e8e6` (ghostty@20abdb50a `build.zig.zon:19-22`; the cached tarball
  `libxev-0.0.0-86vtcwIRFADbH4hk-…`) `src/backend/kqueue.zig:965-968, 973-975, 1488-1510`,
  `src/watcher/async.zig:10-30`, `src/dynamic.zig:34-39`; ghostty `src/global.zig:30`.
- **rustty**: `crates/rustty-xev/src/kqueue.rs` (the FreeBSD `Wakeup`, `Loop::wakeup`,
  `thread_perform`).
- **description**: the W2D row said the stub "fires only on an explicit `Loop::wakeup()`" and that
  an eventfd wakeup "must land upstream first, or be recorded here as an R1 divergence". At the pin
  no caller can make that call: `Loop.wakeup` is not `pub`, its one call site in `kqueue.zig` is
  Darwin-gated, `xev.Async` on FreeBSD is `AsyncEventFd` and never calls it, and Ghostty neither
  names it nor creates an `xev.ThreadPool`. R13 applies only to a reachable defect, so the stub
  stays faithful; it is recorded upstream as UD275 (`kind: latent`) with the eventfd fix the TODO
  names. In the port `Loop::wakeup` is also private with no caller on any target.
- **how found**: task tail-s13, grepping every `wakeup`/`notify` caller in the pinned libxev and in
  ghostty `src/`.
- **test**: `kqueue::tests::freebsd_wakeup_stub_is_the_reference_panic` (FreeBSD-only, type-checked
  from macOS, not run; `VERIFICATION_DEBT.md` V23).
- **state**: CLOSED (task tail-s13).

## N256 — the FreeBSD row's ios/visionos item was stale, and the three pty stream tests ran where the reference skips them

- **id**: N256. Not a ledger row: `.port/phase-2/DEFERRALS.md` FreeBSD row, item 4.
- **severity**: low. **kind**: stale record + skip scope (R14).
- **oracle**: libxev `src/backend/kqueue.zig:29-36` (`.ios, .macos, .visionos => true`, `.freebsd
  => true`); `src/watcher/stream.zig:1145-1150, 1208-1213, 1272-1277` (`switch (builtin.os.tag) {
  .linux, .macos => {}, else => return error.SkipZigTest }` in the three pty tests).
- **rustty**: `crates/rustty-xev/src/lib.rs` (module gates), the module comments of `kqueue.rs`,
  `async.rs`, `net.rs`, `process.rs`, `stream.rs`, `timer.rs`; `stream.rs` `pty_child_to_parent`,
  `pty_parent_to_child`, `pty_queued_writes`.
- **description**: item 4 said rustty gates the kqueue family `any(macos, freebsd)` only.
  `318af0f6` (2026-08-26) had already widened every gate to `macos + ios + visionos + freebsd`; only
  six module comments still said otherwise, and they are corrected. Checking the widened module
  turned up one real gap: the three pty stream tests carried the reference's `.linux, .macos` skip
  only as a comment claiming "this module is macOS-gated", so on FreeBSD and iOS they would have run
  where the reference skips them. They now carry the scope as
  `cfg_attr(all(not(miri), not(any(linux, macos))), ignore = "SkipZigTest: …(R14)")`, with the
  reference's switch kept as a comment in each.
- **verification**: `cargo check -p rustty-xev --tests` for `aarch64-apple-ios` and
  `x86_64-unknown-freebsd` green; `visionos` has no rustup std and is unchecked.
- **state**: CLOSED (task tail-s13).

## N257 — `cli_smoke.rs`'s `version_plus` asserted nothing about the renderer and libxev lines on FreeBSD

- **id**: N257. Not a ledger row: `.port/phase-2/DEFERRALS.md` FreeBSD row, item 6.
- **severity**: low. **kind**: missing test arm.
- **oracle**: `src/cli/version.zig:47-48`; `src/renderer/backend.zig:20-21` (non-Darwin native =>
  `.opengl`); libxev `src/backend.zig:20,38` (FreeBSD's only candidate is kqueue) and
  `src/dynamic.zig:39` (one candidate => the static `Api()`, whose `backend` is `.kqueue`).
- **rustty**: `crates/rustty/tests/cli_smoke.rs` `version_plus`.
- **description**: the test checked those two lines on macOS and Linux only. It now has a
  `cfg(target_os = "freebsd")` arm asserting `renderer.generic.Renderer(renderer.OpenGL)` and
  `libxev        : kqueue`, which is what `crates/rustty-cli/src/version.rs`
  (`renderer_type_name`, `libxev_backend_tag`) produces from `rustty-build-config` and
  `rustty_xev::backend()` on FreeBSD.
- **verification**: `rustty` cannot be type-checked for FreeBSD from macOS (`rustty-os`'s build
  script compiles `cbits/pty.c`, which needs the target's headers), so the new arm was
  compile-checked by flipping its `cfg` to macOS for one `cargo check -p rustty --test cli_smoke`
  (green), then restored. It runs on a FreeBSD host (`VERIFICATION_DEBT.md` V23).
- **state**: CLOSED (task tail-s13).

## N260 — the caller's allocator must outlive every object built from it; the header does not say so

- **id**: N260. Not a ledger row: recorded by the S-7 design study (`.port/parity-closure/S7-ALLOCATOR-DESIGN.md`
  §1.1, §8), filed by task S7-9.
- **severity**: informational. **kind**: reference documentation gap (no port change).
- **oracle**: `src/lib/allocator.zig:78-87` at 20abdb50a: `Allocator.zig()` builds a `std.mem.Allocator` whose
  `.ptr` is the caller's own `GhosttyAllocator*` (`@ptrCast(@constCast(self))`); no copy of the struct is taken,
  and every call re-reads `ctx`/`vtable` through that pointer. `include/ghostty/vt/allocator.h` never states the
  lifetime this implies.
- **rustty**: `crates/rustty-vt/src/alloc.rs` (module doc: `VtAllocator`'s `Some` arm is the caller's own
  `RusttyAllocator` pointer, re-read on every call; "the caller's `RusttyAllocator` must outlive every object
  built from it"); `crates/rustty-lib/src/allocator.rs` `zig()` / `CAllocatorAsZig`. The port matches the
  reference: same pointer, same re-read, same implied contract. `include/rustty/vt/allocator.h` stays verbatim
  (R1), so it is silent too.
- **state**: CLOSED (informational; filed by S7-9).

## N261 — allocator_api2's stable `Vec` has no memcpy/fill specialization (+81% on `screen-clone.render` when swapped naively)

- **id**: N261. Not a ledger row: measured by the S-7 design study (§4 (B) perf, §5), filed by task S7-9.
- **severity**: medium (performance hazard). **kind**: container codegen.
- **what**: allocator_api2 0.2.21's stable `Vec::extend_from_slice` is `extend(iter.cloned())` and `resize` a
  per-element `ptr::write` loop, with no memcpy or fill specialization (std's `Vec` has both). Swapping
  RenderState's std `Vec`s for it cost +80.5% (L1) / +81.0% (L5) on `screen-clone.render` (design §5 table,
  N=9). `Extend` takes the same per-element loop (review S7-12 F8).
- **fix**: `crates/rustty-vt/src/alloc.rs` `CopyVecExt` (`extend_copy`, `resize_copy` and the fallible
  `try_extend_copy`, `try_resize_copy`): a bulk copy / fill after one reservation. The prototype with it
  measured within noise of the baseline (design §5). Gated: clippy `disallowed-methods` bans allocator_api2
  `Vec::{extend_from_slice, resize}` in `crates/rustty-vt/clippy.toml` and `crates/rustty-vt-c/clippy.toml`,
  and G-VTALLOC (`.port/tools/gates/check_vt_alloc_vec_ban.sh`, with a self-test that proves the ban fires).
- **state**: CLOSED (task S7-1; filed by S7-9).

## N262 — a search's allocations are split between the search's allocator and the terminal's

- **id**: N262. Not a ledger row: found by the S-7 design study (§1.2 table, §8), filed by task S7-9.
- **severity**: informational. **kind**: allocator routing (reference behaviour to reproduce).
- **oracle**: `src/terminal/c/search.zig:143-146` (the `searches` registry entry, `putNoClobber(terminal.gpa(),
  ..)`), `src/terminal/search/pagelist.zig:58`, `search/screen.zig:557` and `highlight.zig:71-73` (the three
  `trackPin` sites, on the screen's `PageList`, whose pin pool and pin set are the terminal's), at 20abdb50a.
  Everything else a search allocates goes through the search's own allocator.
- **rustty**: reproduced by task S7-6: the registry entry and the tracked pins are charged to the terminal, the
  rest to the search (NEW-FINDINGS N325, "charged to the TERMINAL's allocator"; tests
  `terminal_search_internals_route_through_the_allocator_handle`,
  `terminal_search_terminal_charged_allocation_failure_sweep`, vt-c
  `search_registry_routes_through_the_terminals_allocator`).
- **state**: CLOSED (task S7-6, N325; filed by S7-9).

## N263 — OSC `Command` text payloads were owned copies where the reference borrows

- **id**: N263. Not a ledger row: found by the S-7 design study (§2, §8), filed by task S7-9.
- **severity**: low. **kind**: port-only allocations (routing them would over-report calls the reference never
  makes; leaving them on the Rust heap leaves memory unaccounted).
- **oracle**: `src/terminal/osc.zig` and `osc/parsers/*.zig` at 20abdb50a: text payloads are slices into the
  parser's buffer or capture, valid until the next parser call.
- **rustty (before)**: about 15 `osc::Command` variants held owned `Vec<u8>` copies (`osc/mod.rs:52-211` at the
  design's base `d33aef69`), plus a deep `Box<Command>` clone per dispatch.
- **fix**: task S7-7 made the command borrow (`osc::Command<'p>`, `ZStr<'p>`, `&'p [u8]`); the lists belong to
  the parser. Recorded as resolved in N320.
- **state**: CLOSED (task S7-7, N320; filed by S7-9).

## N264 — `PageList::init` had lost the reference's `alloc` parameter

- **id**: N264. Not a ledger row: found by the S-7 design study (§2, §8), filed by task S7-9.
- **severity**: low. **kind**: signature fidelity (R1).
- **oracle**: `src/terminal/PageList.zig` `init(alloc, cols, rows, max_size)` and `MemoryPool.init(gen_alloc,
  page_alloc, preheat)` at 20abdb50a.
- **rustty (before)**: `PageList::init(opts)` with no allocator, 336 call sites (almost all tests), and pools on
  the Rust heap.
- **fix**: task S7-2 restored `PageList::init(alloc, opts)` and `MemoryPool::init(alloc, page_backing_alloc(),
  preheat)` (`crates/rustty-vt/src/pagelist/mod.rs`); S7-3 routed the pools through the handle (N272).
- **state**: CLOSED (tasks S7-2/S7-3; filed by S7-9).

## N265 — the reference's `test "face metrics"` never checks anything: its backend guard is inverted

- **id**: N265. Not a ledger row: found by G2 once the `font` crate key was graded with its
  R4 relocation (task tail-s8c).
- **severity**: low. **kind**: reference test-weakness (R13: stays faithful; needs a
  `UPSTREAM-DEFECTS.md` entry, whose UD id the controller assigns — this batch had none).
- **oracle**: `src/font/Collection.zig:1396-1523` at 20abdb50a. `:1397-1398`: "The web canvas
  backend doesn't calculate face metrics, only cell metrics" followed by
  `if (options.backend != .web_canvas) return error.SkipZigTest;`, so the test is skipped on
  every backend except the one its comment excludes. On `web_canvas`, every expected-value
  switch (`:1438-1502`) takes its `.web_canvas => unreachable` prong. The switches also have no
  `freetype_windows` prong, so they would not compile if analyzed on that backend.
- **rustty**: `crates/rustty-font-shaper/src/collection.rs` `face_metrics`. Before this task the
  body was only comments, and the test had zero assertions. The body is now ported live. The skip
  is kept as `cfg_attr(not(wasm32), ignore)`, because `options::BACKEND` is `WebCanvas` exactly on
  wasm32 (R14 form 1).
- **evidence**: run with `--ignored` on macOS (the `coretext` backend), the live body passes
  against the reference's own coretext expected values, for both CozetteVector and Geist Mono.
  Negative control: changing the narrow font's coretext `cell_width` from 7.3828125 to 7.5 makes
  it fail at the `expect_approx_equal` assertion. The `fontconfig_freetype` values: run with
  `--ignored` on Linux (task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26) the body fails at the narrow font,
  `expected approximately 8, found 7` (N285 / UD281's stale FreeType expectation).
- **proposed upstream fix**: `if (options.backend == .web_canvas) return error.SkipZigTest;`, plus a
  `freetype_windows` prong or an `else` in the four switches.
- **state**: RECORDED as `UPSTREAM-DEFECTS.md` UD276 (controller, 2026-09-24).

## N266 — the font backend build option is folded to the per-OS default: the `coretext_freetype`, `coretext_harfbuzz`, `coretext_noshape` and `freetype` arms are not live

- **id**: N266. Not a ledger row (task tail-s8c, residual of the `font` done-flip).
- **severity**: medium. **kind**: R8 build-switch fold / R1 missing code paths.
- **oracle** (20abdb50a): `src/build/Config.zig:191-195` (`-Dfont-backend`, which defaults to
  `FontBackend.default(target)`); `src/font/main.zig:50`
  (`.backend = ... build_config.font_backend`). `.github/workflows/test.yml:1383-1428`
  (`build-macos-freetype`) builds AND tests `-Dfont-backend=coretext_freetype` on macOS, so
  Ghostty at the pin can be built with a non-default backend on this platform. There are 53
  `options.backend` / `font_backend` / `backend.has*` sites in 18 files: `font/{face,library,
  shape,discovery,DeferredFace,Collection,CodepointResolver,main}.zig`,
  `font/face/coretext.zig`, `font/shaper/{coretext,harfbuzz}.zig`, `apprt/embedded.zig`,
  `cli/version.zig`, `crash/sentry.zig`, `global.zig`, and `build/{Config,SharedDeps}.zig`.
- **rustty**: `crates/rustty-font-shaper/src/lib.rs` `options::BACKEND = Backend::default()` (a
  `cfg(target_os)` decision with no override); `face/mod.rs:88-93`, `shape.rs:135-140` and
  `discovery.rs:71-83` choose `Face`/`Shaper`/`Discover` by `target_os`. `deferred_face.rs`
  records `loadCoreTextFreetype` as a comment only. There is no cargo feature that selects a
  backend. The existing `freetype`/`harfbuzz`/`fontconfig` features gate link edges only.
- **stale records**: `docs/parity-audit/src-font.md:15` ("`loadCoreTextFreetype` is
  not-applicable") and `docs/superpowers/specs/2026-08-25-build-system-gap-audit.md:539-542` (it
  re-classifies `-Dfont-backend` as "correctly absent" on the premise that `rustty-font` has no
  FreeType/fontconfig/CoreText dependency, which is false for `rustty-font-shaper`) both rest
  on the fold.
- **what closing needs**: one mutually exclusive cargo feature per backend on
  `rustty-font-shaper` (default = the per-OS default, so existing builds are unchanged), with
  `options::BACKEND` derived from it. Every type alias and `cfg(target_os)` backend gate must be
  re-keyed on that `BACKEND` (face, shaper, discovery, library, deferred-face arms, collection,
  resolver, the coretext face's `hasHarfbuzz` shaper font, and the consumers in apprt/cli/crash/
  global). `loadCoreTextFreetype` has to be ported live. A CI lane (or local run) must build and
  test `coretext_freetype` on macOS, as the oracle does. This is a multi-crate port, not a
  gate-tail item.
- **state**: **CLOSED** (triage T-1, 2026-09-28): closed by task tail-s14/tail-s14b (merged `1d886967`; `font` PORT STATUS done in `6abf60df`), as the "closed by" record below says. The only remainder is N286's `fontconfig_freetype` on Windows. Was: OPEN. `font` stays `PORT STATUS: in progress` until it closes.
- **closed by** (task tail-s14, 2026-09-24): CLOSED.
  - **option**: `-Dfont-backend` is `rustty-build-config`'s `font-backend-{freetype,
    freetype-windows,fontconfig-freetype,coretext,coretext-freetype,coretext-harfbuzz,
    coretext-noshape}` features (at most one — its build script refuses two; none = the
    per-target `Backend.default`, so every existing build is unchanged). Its build script
    publishes the resolved tag as `links` metadata; `rustty-font-shaper`'s build script turns it
    into `rustty_font_backend="<tag>"` + `rustty_font_has_{freetype,coretext,fontconfig,
    harfbuzz}` (`backend.zig`'s `has*`), and `options::BACKEND` is read from that cfg, with a
    const assert that it equals `build_config::font_backend()`. `rustty` and `rustty-apprt-c`
    forward the same features. A backend the port does not build on the target is refused by
    name (N286).
  - **sites made live**: `Face`, `FaceError`, `FreetypeLoadFlags`/`Options`, `Shaper` (+ a
    `ShaperError` alias for callers), `Library`/`InitError`, `Discover` (+ `DISCOVER_IS_VOID` for
    every `Discover == void` site in SharedGridSet, CodepointResolver (fallback and override —
    actually applied in tail-s14b), list_fonts and show_face (tail-s14b), and
    `DISCOVER_WARMUP` for App's `@hasDecl(Discover, "warmup")`), the `DeferredFace` payloads
    (`ct`/`fc`/`win`/`wc` keyed on their own predicates; the `.freetype` no-payload arms),
    `loadCoreTextFreetype`, `discoverCodepoint`'s `hasFreetype()` early return, the CoreText
    face's `harfbuzz_shaper` arms (`hb_font` via `hb_coretext_font_create`, `setScale`,
    `errdefer`/`deinit` destroy), the noop shaper as the `.coretext_noshape` `Shaper`, the apple-
    emoji `isDarwin() and Discover != void` pair, `apprt/embedded.zig`'s quicklook `backend !=
    .coretext => null`, `Surface.DerivedConfig`'s `FreetypeLoadFlags != void` arm (core and
    `+show-face`), and every backend-keyed test gate. `+version`/sentry/global log read
    `build_config.font_backend`, which now reflects the option. The renderer and core read the
    same `links` metadata in their build scripts for their few type-level uses.
  - **evidence (macOS, 2026-09-24)**: the G-FONTBACKEND lane (`check_font_backends.sh`, HARD in
    ci.sh — the reference's `build-macos-freetype` job) is GREEN: arm 0 (a default build against
    `RUSTTY_EXPECT_FONT_BACKEND=coretext_freetype` is RED for the stated reason); font / font-
    shaper / renderer lib+integration tests pass under `coretext_freetype` 88 / 135 (+6 ignored) /
    117+3, `coretext_harfbuzz` 88 / 130 (+8) / 117+3, `coretext_noshape` 88 / 102 (+7) / 117+3,
    `freetype` 88 / 115 (+7) / 117+3 (default `coretext`: 88 / 136 (+4) / 118+3); the workspace
    builds under `coretext_freetype`. `cargo clippy --workspace --all-targets -D warnings` is clean
    for all five macOS backends. core + cli + apprt-c tests pass under `coretext_freetype`. The
    built `rustty` binary reports `font engine : .coretext_freetype` and `+list-fonts` / `+show-
    face` work. New tests: `backend_selection_tests::selectors_follow_options_backend` (restates
    each `switch (options.backend)` against the resolved alias types), three
    `load_coretext_freetype_*` tests + UD280's, and two `coretext_harfbuzz` `hb_font` tests;
    negative controls (each run alone, reverted): no percent-decode → FreeType
    `CannotOpenResource`; `FontHasNoFile` → `OutOfMemory` fails its test; dropping the
    `errdefer face.deinit()` → "the face was deinit'ed on the error path"; dropping the CoreText
    face's `hb_font.destroy()` → "the HarfBuzz font is destroyed on the error path, not leaked".
    Cross type-checks (`cargo check --tests`, zig cc sysroots): `aarch64-unknown-linux-gnu` default
    and `freetype`, `x86_64-pc-windows-gnu` default (`freetype_windows`, after N287) and
    `freetype`, `wasm32-unknown-unknown` (font-shaper, web-canvas, wasm).
  - **remainder** (not N266): Linux/Windows arms type-checked only (no VM run this batch); the
    `face metrics` FreeType arm disagrees on one value (N285); combinations refused by name
    (N286). The `font` crate key's `done` flip is left to the controller (G6 Miri/ASan lanes).
  - **tail-s14b (review of S-14)**: three sites the S-14 record called live were not: the
    CodepointResolver fallback and override `Discover == void` pair (the edit had not been
    applied) and `+show-face`'s void arm (still `cfg(wasm32)`, M1; now `DISCOVER_IS_VOID`, with
    tests on both arms). Also keyed on the backend: SharedGridSet's `@hasDecl(Discover,
    "discoverExactFamily")` (CoreText only), the `fc` module / `toFcPattern` / Fontconfig tests
    (`hasFontconfig()`), cli's `DerivedConfig` load-flags test (`hasFreetype()`, via a cli
    `build.rs`). `fontconfig_freetype` and `freetype_windows` are now built and tested on macOS
    (N286). The core and cli build scripts' cfgs are const-asserted against `options::BACKEND`.
    G-FONTBACKEND gains the manifest lint (arm L), per-backend clippy, and core/cli/apprt-c
    tests under `coretext_freetype` and `freetype`. N285 is settled (stale reference values,
    UD281).

## N270 — `GlobalZigAllocator::resize` reported success for any shrink, so the later free reached `dealloc` with a smaller layout

- **id**: N270. Not a ledger row: review S7-12 F4 (confirmed by probe), fixed in task S7-3.
- **severity**: medium. **kind**: port soundness defect (not a reference defect).
- **oracle**: Zig 0.16 `lib/std/heap.zig` `c_allocator_impl.resize`: `if (new_len <= memory.len)
  return true; // in-place shrink always works`, otherwise `new_len <= malloc_size(ptr)`, or `return
  false` where no usable-size query exists. Legal there because C `free` takes no size.
- **rustty**: `crates/rustty-lib/src/allocator.rs` `GlobalZigAllocator::resize` (backs
  `c_allocator()`, `default(NULL)`, and the vt-c test allocators).
- **description**: the port maps `std.heap.c_allocator` onto the Rust global heap (the documented
  mapping, shared with `rustty_vt::alloc::VtAllocator`'s Global arm and zigstd's `impl ResizeInPlace
  for Global`). `resize` answered `new_len <= memory.len` without moving the block, so the caller's
  later `free(new_len)` called `std::alloc::dealloc` with a smaller `Layout` than `alloc` used:
  undefined behaviour under the `GlobalAlloc` contract, harmless only because the macOS/glibc
  `System` allocator ignores the size. A zigstd arena over the handle's C arm (S7-3/S7-5) reaches it
  through `reset(.retain_with_limit)`.
- **decision**: `resize` now succeeds only for `new_len == memory.len`. Backing the allocator with
  libc `malloc`/`posix_memalign`/`free` + `malloc_size` would reproduce the reference's in-place
  answers, but it would split the port's single "NULL allocator" into two heaps (the VtAllocator
  Global arm stays the Rust heap) and add a libc surface Miri cannot model (`malloc_size`). Refusing
  is within the Zig allocator contract (the reference's own no-usable-size arm returns `false`);
  callers fall back to `remap` (`realloc`) or alloc+copy+free. `remap` is unchanged.
- **verification**: `crates/rustty-lib/tests/global_zig_allocator_resize.rs` (own binary, a
  size-checking `#[global_allocator]`): alloc 64, shrink-resize, free with the length the contract
  implies; asserts zero layout mismatches, then that the shrink was refused. Negative control
  (`new_len <= memory.len` reinstated): fails with "left: 0, right: 1" mismatches.
- **siblings**: the same answer over the Rust heap sat in `rustty_vt::sys::GlobalRawAllocator` (the
  kitty PNG decode seam's allocator) and in rustty-datastruct's test-only `GlobalAllocator`; both are
  fixed as N273. With N273 no Rust-heap Zig-style allocator in the workspace reports an in-place shrink.
- **state**: CLOSED (task S7-3).

## N271 — six port-only tripwire stand-ins for the reference's `FailingAllocator` tests are retired

- **id**: N271. Not a ledger row: S-7 design §7 S7-3 ("the ported reference `FailingAllocator` tests,
  assertion for assertion"; "retires about 66 adaptation notes").
- **severity**: low. **kind**: test fidelity (R1).
- **oracle**: `PageList.zig` tests "Builder finish is transactional on allocation failure", "PageAllocation
  allocation failure leaves list unchanged", "incremental compression advances after allocation failure",
  "preserved page keeps compressed storage"; `compress/Page.zig` "rejects a representation without
  savings"; `Screen.zig` "implicit hyperlink ID wraps"; `Terminal.zig` "setPwd/setTitle preserves a
  sentinel on allocation failure", "resize preserves tabstops on allocation failure";
  `stream_terminal.zig` "semantic failure is sticky while processing continues"; `snapshot/screen.zig`
  SCREEN restoration; `c/terminal.zig` "get vt_processing_error".
- **rustty**: the tests of the same names in `crates/rustty-vt/src/{pagelist/tests.rs, compress/page.rs,
  screen.rs, terminal.rs, stream_terminal.rs, snapshot/screen.rs}` and `crates/rustty-vt-c/src/terminal.rs`.
- **description**: while the PageList pools, compressed pages, cursor hyperlink and pwd/title lived on the
  Rust heap, these tests injected their failures through port-only tripwire modules (`FinishFp`,
  `AllocatePageFp`, `PagePreservingStateFp`, compress `InitFp`, `StartHyperlinkFp`, `SetStringFp`) that
  the reference does not have. With those allocations on the allocator handle (S7-3), each test now runs
  over `alloc::testing::FailingAllocator` (or `alloc::failing()`) exactly as written, asserting
  `has_induced_failure` where the reference does, and the six modules are deleted. The reference's own
  tripwires (`init_tw`, `initPages_tw`, `compressPage_tw`, Tabstops/Terminal `resize_tw`) stay.
- **deviation (test-only)**: the setPwd/setTitle tests also set `resize_fail_index = resize_index`. The
  reference's inner `testing.allocator` (DebugAllocator) cannot remap a 3- or 5-byte size-class slot to
  the grown capacity, so its growth reaches the failing `alloc`; the port's inner heap remaps with
  `realloc`, which would move and succeed. PORT NOTE at both sites.
- **state**: CLOSED (task S7-3).

## N272 — S7-3 routing: what goes through the allocator handle, what is bound at construction, and what is left

- **id**: N272. Not a ledger row: records the S7-3 scope decisions (S179-F2 closes in S7-4).
- **severity**: informational. **kind**: allocator routing (T1/T2/T3 of the S-7 design).
- **routed (T1, with T2 at every reference `try`)**: PageList node + pin pools, every tracked-pin set,
  the compressed-page encoding, `Node.pagePreservingState(alloc)` buffers, the clone's pools and
  `TrackedPinsRemap`; `ScreenSet`'s `alloc.create(Screen)`; the cursor hyperlink (`alloc.create` + `dupe`);
  `Tabstops.dynamic_stops`; `Terminal.pwd`/`title`; the glyph glossary map; the kitty drag-and-drop state
  (box, MIME lists, offered list, held drop copies). vt-c `new` passes `lib.alloc.default(alloc_)` into
  `Terminal.init`. Page memory stays on the page allocator.
- **per-call vs bound**: `pwd`/`title` keep the reference's per-call allocator
  (`alloc::ArrayListUnmanaged`, grown through `gpa()`; the reference's C-API test swaps the active
  screen's allocator to make that growth fail). The glossary map, the DnD state and the Tabstops box bind
  the allocator when created, as allocator_api2 containers do; the reference passes the same allocator
  per call, and the DnD methods debug-assert that it is the bound one.
- **left for S7-4 (not routed here, on the Rust heap as before; no new abort)**: the glyph `Entry`
  payloads (the `Outline` decoded in rustty-font, and the decode scratch), vt-c `from_decoded` (the
  terminal comes from the snapshot decoder, whose allocator S7-4 routes; `from_decoded` now frees with
  the decoded terminal's own `gpa()`), and the inspector's two `pagePreservingState(std.heap.page_allocator)`
  calls, which take the Rust heap handle (the handle has no page-allocator arm; outside the C API).
- **T3 (call shape) residuals**: the zigstd `AutoArrayHashMapUnmanaged` is an allocator_api2 `Vec` of
  entries plus a hashbrown index (Zig: a `MultiArrayList` plus an index header above a linear-scan
  threshold), so the counts differ; the routing tests pin the port's (PageList.init: pools 4 + pin set 2).
  `Hyperlink` is generic over its byte storage (`Vec<u8>` default; `AllocBytes` for the cursor copy);
  `startHyperlink`'s port-only copies (the up-front and per-attempt `dupe`) stay on the Rust heap, and the
  extra `insert_arg` copy is gone (the per-attempt value is moved into the page insert).
- **verification**: routing tests with counting allocators and exact counts per family
  (`pagelist_internals_route_through_the_allocator_handle`, `screen_internals_route_…`,
  `terminal_internals_route_…`, `glossary_map_routes_…`, `dnd_state_routes_…`), each with a negative
  control that routes one site to Global; `allocation_failure_sweep_through_the_c_api` (vt-c).
- **state**: CLOSED (task S7-4 routes the "left for S7-4" items and closes S179-F2; see N274).

## N273 — `sys::GlobalRawAllocator::raw_resize` reported success for any shrink (the sibling of N270)

- **id**: N273. Not a ledger row: review S7-3 F1. Sibling of N270, fixed in task S7-3b.
- **severity**: medium. **kind**: port soundness defect (not a reference defect).
- **oracle**: as N270 (Zig 0.16 `c_allocator.resize`); the reference hands the decoder a `LimitedAllocator`
  over its general-purpose allocator (`kitty/graphics_image.zig` `decodePng`, `590d669c4`).
- **rustty**: `crates/rustty-vt/src/sys.rs` `GlobalRawAllocator::raw_resize`; the test-only copy
  `crates/rustty-datastruct/src/limited_allocator.rs` `tests::GlobalAllocator::raw_resize`.
- **description**: `GlobalRawAllocator` is the Rust-heap `RawAllocator` behind the kitty PNG decode seam
  (`graphics_image.rs` `LimitedAllocator::init(sys::GlobalRawAllocator, ...)`, handed to the embedder's
  decoder through vt-c `sys.rs`). `raw_resize` answered `new_len <= memory.len` without moving the block,
  so a decoder that shrank a buffer with `resize` and freed it with the new length reached
  `std::alloc::dealloc` with a smaller `Layout` than `alloc` used: the same undefined behaviour as N270.
- **fix**: both now succeed in place only for `new_len == memory.len`, with N270's reasoning (PORT NOTE
  at the production site). Refusing is within the Zig allocator contract; `LimitedAllocator` forwards.
- **verification**: `crates/rustty-vt/tests/global_raw_allocator_resize.rs` (own binary, a size-checking
  `#[global_allocator]`): alloc 64, shrink-resize, free with the contract's length, directly and through
  `LimitedAllocator`; asserts zero layout mismatches, then that the shrinks were refused. Negative control
  (`new_len <= memory.len` reinstated in sys.rs): fails with "left: 0, right: 2" mismatches.
- **state**: CLOSED (task S7-3b).

## N274 — S7-4 routing: kitty graphics, APC/DCS, stream handler, clipboards and the vt-c wrapper, through the allocator handle

- **id**: N274. Not a ledger row: records the S7-4 scope decisions (closes S179-F2 with N272).
- **severity**: informational. **kind**: allocator routing (T1/T2/T3 of the S-7 design).
- **routed (T1)**: kitty graphics: the command parser's payload and `Command.data`, `LoadingImage`
  (box, data, zlib output, file and shared-memory reads), `Image` data, the animation box, frame list and
  frame data, the RGBA conversion, the image and placement maps (hashbrown over the handle), the scroll
  `restores` list (screen's allocator); the PNG decode seam's `LimitedAllocator` wraps the caller's
  allocator, as at the pin. APC: the kitty/glyph parsers and the unknown capture (the `feed` allocator),
  the glyph `Outline` (rustty-font `Outline<A>`, `decode_limited_in`). DCS: the tmux control buffer. The
  stream handler: the OSC 52 decode scratch, the kitty clipboard metadata/payload, grants, write state and
  committed contents, all eight `stackFallback` response writers (`StackFallbackWriter<N>` over `gpa()`),
  `paste.Context.alloc`, and the stream continuation tracker (`Tracker::init(alloc, ..)`, `Stream::init_in`;
  since S7-4b an `alloc::ArrayListUnmanaged`, so its first reservation is `growCapacity(4096)`, 6272 bytes
  on 64-bit targets, and later growth rounds the same way, as the reference's `ArrayList` does).
  vt-c: the snapshot decoder builds the terminal over the caller's allocator (`from_decoded` asserts it),
  the `tracked_grid_refs`/`searches` maps are `AArrayHashMap`s bound in `wrap`, `set(continuation_max_bytes)`
  builds the tracker over `gpa()`, and the clipboard trampolines and `terminal_paste` use
  `alloc::StackFallback<N>` (a port of `std.heap.stackFallback`) over `gpa()` with the reference's sizes
  (128, 256, 4-on-stack, 256). The inspector's two `pagePreservingState(std.heap.page_allocator)` calls
  take `alloc::page_allocator()`, a handle over `rustty_os::page_allocator`.
- **fallibility (T2)**: each reference `try` or `catch` in these families is reachable: the clipboard
  handlers return `Result` and their dispatch arms log; the write state, grants and loading image fail
  with `OutOfMemory`; animation frames and canvases answer `ENOMEM`; the tracker marks itself broken; the
  vt-c write trampoline replies `io_error`, the read trampolines log and return, `terminal_paste` returns
  `RUSTTY_OUT_OF_MEMORY`. S7-4b (review F1): the PNG decode seam hands the decoder's own buffer over
  (`sys::Image.data` is a `RawSlice` on the decode allocator, as the reference's `[]u8`), and `decodePng`
  copies it into `alloc` fallibly (`try_reserve_exact`) before freeing it through the decode allocator;
  the infallible Rust-heap copy outside the limit, which aborted where the reference answers ENOMEM, is
  gone.
- **per-call vs bound**: the reference passes the allocator per call; containers bind it on first growth
  (`alloc::bind`/`bind_map`) or at construction and debug-assert it is the same one on later calls.
- **T3 (call shape) residuals**: the reference's per-transaction arenas are per-value `Bytes`;
  `toOwnedSlice` hands the list over without shrinking it; `StackFallbackWriter` spills when the payload
  crosses `N` (Zig's `Writer.Allocating` can reach the fallback a little earlier and grows by
  `growCapacity`); `alloc::page_allocator()` carries the PageList's VM tag on Darwin
  (the reference's `std.heap.page_allocator` is untagged); `withoutData` is a field copy with no
  animation. Port-only transient allocations that the reference does not make (response message strings,
  `format!` report strings, placement/image key snapshots taken before mutating a map, tmux notification
  field copies) stay on the Rust heap: they are not caller allocations in the reference, so they are
  not routed. zlib's inflate state comes from zlib's own `malloc` (`zalloc` null), where the reference
  inflates with `std.compress.flate` and a window on the stack: not a caller allocation in either, but
  the port's is a C heap allocation per zlib-compressed image.
- **reachability**: the read-request trampoline's `stackFallback(128)` never spills from the stream: at most
  `max_read_mimes` (4) MIME types, 64 bytes, reach it. It is routed as written; its spill arm is not
  exercised by a test.
- **verification**: routing tests over counting allocators. At S7-4 four routed sites had no test that
  noticed a revert to Global (review S7-4 NC-C to NC-F: the DCS tmux buffer, the OSC 52 read-reply
  writer, the vt-c `searches` map, the paste reader's writer), and only the OSC 4 writer's spill was
  tested. S7-4b added the missing tests; each of the four controls and a Global revert of each writer
  below now fails its test:
  - kitty graphics: `kitty_graphics_route_…`, `kitty_chunked_loading_routes_…`,
    `image_load_png_decode_routes_…` (peak bytes: the decoder buffer and the copy are live together),
    rustty-vt `default_decoder_hands_over_its_buffer`;
  - APC/DCS: `apc_builders_route_…`, `tmux_buffer_routes_through_the_hook_allocator`,
    `tmux_init_options_reports_allocation_failure` (through a `FailingAllocator` handle);
  - stream handler: `grants_route_…`, `write_state_routes_…`, `metadata_and_payload_route_…`,
    `stream_handler_internals_route_…` (OSC 52 scratch, clipboard state, the OSC 4 writer),
    `response_writers_spill_through_gpa` (OSC 52 read reply, Kitty DATA reply, CSI 21 t, Kitty color,
    OSC 72 serving: each large response costs exactly one spill over the small one),
    `paste_reader_contents_route_through_gpa`, `tracker_routes_…`;
  - vt-c: `decoded_terminal_internals_route_…`,
    `decoded_terminal_continuation_tracker_is_on_the_callers_allocator`, `continuation_tracker_routes_…`,
    `clipboard_trampolines_route_…`, `terminal_paste_mimes_route_through_the_stack_fallback`,
    `search_registry_routes_through_the_terminals_allocator`, the grid-ref registry test.
  Not covered by a routing test: the Kitty graphics response writer (1024; a status and an echoed id,
  which "virtually always fit on the stack", as the reference says) and the DA writer (128), whose
  spill arms no test input reaches, and the read-request spill arm (above). The vt-c C-API
  failure sweep (alloc and remap loops; the clean run fails nothing and a refused remap makes no call
  report OUT_OF_MEMORY) runs over a corpus that reaches these families and a search
  (`sweep_corpus_reaches_the_s7_4_families`); `allocation_failure_sweep_through_snapshot_decode` sweeps a
  snapshot whose unfinished 7000-byte CSI makes the decode remap (its remap loop had exercised nothing).
- **state**: CLOSED (task S7-4; review fixes S7-4b).

## N275 — the kitty graphics payload was cloned before its in-place base64 decode

- **id**: N275. Not a ledger row: found while routing the kitty command parser (S7-4).
- **severity**: low. **kind**: port-only allocation (not a reference defect).
- **oracle**: `kitty/graphics_command.zig` `decodeData`: decodes `self.data.items` on top of itself
  ("the encoded size is larger, and any bytes in areas that are written to will have already been used").
- **rustty**: `crates/rustty-vt/src/kitty/graphics_command.rs` `decode_data`; `crates/rustty-simd/src/base64.rs`
  `decode_in_place`.
- **description**: the borrow checker forbids one `Vec` as both source and destination, so the port
  cloned the whole payload (up to the protocol's maximum) on the Rust heap and decoded the clone into the
  original. Every transmitted chunk paid a second full-size allocation the reference does not make.
- **fix**: `simd::base64::decode_in_place(buf, max_len)` hands the kernel the same pointer for input and
  output, as the reference's call does, and `decode_data` truncates in place. The scalar lane (wasm32,
  Miri) decodes in place too since S7-4b (review F5): `Base64Decoder::decode_in_place` runs the scalar
  decoder over one buffer through raw pointers (each step reads its source bytes before writing, and a
  write ends before the next read starts), so no lane copies the payload.
- **verification**: `decode_in_place_matches_decode` (same bytes and errors as a separate-buffer decode,
  SIMD lane; the scalar lane under Miri); `scalar_decode_in_place_matches_scalar_decode` (the scalar lane
  on every target, lengths 0..200, padded and invalid; negative control: a destination 4 bytes into the
  buffer fails it); the kitty routing tests count no copy.
- **state**: CLOSED (task S7-4).

## N276 — `image_by_id` and `image_by_number` returned a deep copy of the stored image

- **id**: N276. Not a ledger row: found while routing kitty image storage (S7-4).
- **severity**: low. **kind**: port-only allocation (not a reference defect).
- **oracle**: `kitty/graphics_storage.zig` `imageById`/`imageByNumber` return `?Image` by value: a shallow
  struct copy that shares the stored pixel data.
- **rustty**: `crates/rustty-vt/src/kitty/graphics_storage.rs`, `crates/rustty-vt/src/kitty/graphics_exec.rs`.
- **description**: the port returned `Option<Image>` through `.cloned()`, which copied the pixel data and
  the animation's frames: a full-image allocation on every display, delete or frame lookup, through the
  storage's allocator once that is the caller's.
- **fix**: both return `Option<&Image>`. The display path, which the reference gives a value copy and
  then mutates the stored image, takes `Image::withoutData` (a metadata copy, no allocation); the delete
  sites collect `(image id, rect)` before mutating.
- **verification**: `kitty_graphics_route_through_the_allocator_handle` pins the display step's count
  (`a=p`: exactly one allocation, the placements map); the existing display/delete tests pass unchanged.
- **state**: CLOSED (task S7-4).

## N277 — six more port-only tripwire stand-ins for `FailingAllocator` tests are retired

- **id**: N277. Not a ledger row: S-7 design §7 (the sibling of N271 for the S7-4 families).
- **severity**: low. **kind**: test fidelity (R1).
- **rustty**: `CLIPBOARD_CONTENTS_TW`, `KITTY_CLIPBOARD_TW` and `COLOR_OPERATION_TW` (stream_terminal.rs),
  `RGBA_FROM_FORMAT_TW` (graphics_pixel.rs), `SCROLL_MARGINS_TW` (graphics_storage.rs) and `TRACKER_TW`
  (stream_continuation.rs) are deleted.
- **description**: with these allocations on the handle, the tests they served run over
  `alloc::testing::FailingAllocator` or `alloc::failing()` as the reference writes them:
  `clipboard_write_allocation_failure_is_ignored`, `kitty_clipboard_allocation_failure_is_ignored`,
  `semantic_failure_is_not_set_by_a_color_operation_failure`,
  `scroll_margins_restores_append_failure_is_logged_and_skipped`, the `rgba_oom_sweep` tests,
  `tracker_reports_writer_failure_and_defers_allocation_failure`,
  `stream_continuation_lifecycle`'s failing stream and `stream_continuation_allocation_failure_recovers`
  (no longer `cfg(debug_assertions)`-only).
- **deviation (test-only)**: `stream_continuation_allocation_failure_recovers` also sets
  `resize_fail_index = resize_index`, as N271's setPwd/setTitle tests do: the reference's inner
  `testing.allocator` cannot remap the `growCapacity(4096)`-byte reservation to the 12 KiB suffix, so the growth reaches the failing
  `alloc`, while the port's inner heap remaps with `realloc`. Without it the test fails (checked).
- **state**: CLOSED (task S7-4).

## N278 — threads the reference spawns with `std.Thread.spawn(.{}, ..)` had Rust's 2 MiB stack, not Zig's 16 MiB

- **id**: N278. Not a ledger row: review S7-4 F6.
- **severity**: low. **kind**: runtime-environment fidelity (not a reference defect).
- **oracle**: Zig 0.16 `std/Thread.zig`: `SpawnConfig.stack_size = default_stack_size`, `default_stack_size
  = 16 * 1024 * 1024`. Every reference spawn uses `.{}`: `Surface.zig` (renderer, io), `termio/Exec.zig`
  (read thread, gather thread), `terminal/search/Thread.zig`, `crash/sentry.zig`, `App.zig` (font and
  renderer warmups), `font/shaper/coretext.zig` (CF release), `os/flatpak.zig`, `os/open.zig`.
- **rustty**: `rustty_zigstd::thread::{DEFAULT_STACK_SIZE, spawn_config}`; the spawn sites in
  rustty-core (`app.rs`, `surface.rs`), rustty-termio (`exec.rs`), rustty-crash (`sentry.rs`),
  rustty-font-shaper (`shaper/coretext.rs`) and rustty-os (`flatpak.rs`, `open.rs`).
- **description**: the port spawned these threads with `std::thread::Builder::new()`, Rust's 2 MiB
  default. Code the reference sizes against 16 MiB ran on an eighth of it: notably the glyph protocol's
  payload decode, whose `stackFallback(64 KiB)` scratch runs on the io-reader thread. In a debug build
  the port's by-value `StackFallback::new` also copied that 64 KiB through temporaries: a glyph
  registration overflowed a 192 KiB stack (fitting in 256 KiB), and `#[inline(always)]` made it worse.
- **fix**: `spawn_config()` (`Builder::new().stack_size(16 MiB)`) at each site above.
  `StackFallback::new_in_place` builds the 64 KiB scratch in its slot without moving it: a debug
  registration now fits in about 80 KiB (release: 64 KiB). Embedders calling the vt-c API from their own
  threads bring their own stacks, as with the reference.
- **verification**: `glyph_registration_fits_a_128k_stack` (APC feed, request, `Entry::init` on a
  128 KiB thread; negative control: the by-value `new` overflows it, SIGABRT). Measured on aarch64
  macOS by running it at 64..256 KiB (`RUSTTY_GLYPH_STACK`).
- **state**: CLOSED (task S7-4b).

## N280 — `macosLogger`'s per-scope `os_log` cache (upstream `3225e9ebb`) was never ported

- **id**: N280. Not a ledger row: found by G3b while closing bin's comment coverage (task tail-s8d).
- **severity**: low (performance; identical unified-log output). **kind**: missing upstream change.
- **oracle**: `src/main_ghostty.zig:170-199` `macosLogger`, called from `logFn` at `:143`.
- **rustty (pre-fix)**: `crates/rustty-core/src/logging.rs` `log_with` created and released an
  `os_log` object on every log call, under the pre-`3225e9ebb` comment "Initialize a logger. This is
  slow to do on every operation but we shouldn't be logging too much." — which the pin no longer
  has. `docs/parity-audit/src.md` recorded the difference as an accepted equivalence, but the
  reference's reason for the change (os_log creation shows up in startup profiles) is a behaviour
  the port simply lacked; it was a missed upstream commit, not a decision.
- **fix**: `macos_logger(scope)` caches one `Log` per scope for the process lifetime: an acquire
  load, create, `compare_exchange(null, created, AcqRel, Acquire)` and release of the losing logger,
  as the reference does. The reference's comptime-per-scope static becomes a per-scope slot from a
  mutex-guarded registry (Rust scopes are runtime strings). The seven reference comment lines are
  restored.
- **verification**: `logging::tests::macos_logger_is_created_once_per_scope` (same scope returns the
  same object, different scopes differ, 8 racing threads settle on one). Negative control: a
  per-call `Log::create` in `macos_logger` fails it with "same scope must reuse the cached logger".
- **state**: CLOSED (task tail-s8d).

## N281 — `class/config.zig` `getMut` had no port and no record

- **id**: N281. Not a ledger row: S-8b report concern 2.
- **severity**: low. **kind**: unrecorded non-port.
- **oracle**: `src/apprt/gtk/class/config.zig:94-100` `getMut`. No caller anywhere in `src/` at the
  pin (`git grep getMut 20abdb50a -- src`: the definition only).
- **rustty**: `crates/rustty-apprt/src/gtk/class/config.rs` has `get` only.
- **disposition**: recorded NON-PORT, `.port/phase-4/NON_PORTS.md` (a `&mut CoreConfig` from a
  shared, refcounted GObject handle is unsound without changing the storage to a `RefCell` and
  every `get` caller to a guard). Its three doc lines are `no-counterpart` rows of
  `.port/parity-closure/COMMENT-NOT-CARRIED.tsv` (decl `Config.getMut`).
- **S-8e (review)**: the record now also names the declined sound alternative, `unsafe fn
  get_mut(&self) -> *mut CoreConfig` over `UnsafeCell` storage, and why (an uncheckable aliasing
  contract for a function nothing calls; it is the shape to use if a caller appears).
- **state**: CLOSED (task tail-s8d; record completed tail-s8e).

## N282 — `gsettings.zig` `getAlloc` had no port and no record

- **id**: N282. Not a ledger row: S-8b report concern 2.
- **severity**: low. **kind**: missing function (no caller in the reference GTK tree at the pin).
- **oracle**: `src/apprt/gtk/gsettings.zig:50-57` `getAlloc` + `getImpl`'s `[]const u8` arm `:69-74`.
- **fix**: `crates/rustty-apprt/src/gtk/gsettings.rs` `get_alloc(key) -> Option<SettingValue>`, with
  its four reference doc lines restored verbatim. S-8e (review): like the reference it accepts every
  key (`SettingValue::{Bool, CInt, Str}` reifies `?key.Type()`), and the string arm reads the raw
  `GValue` (`property_value` + `g_value_get_string` + `CStr::to_bytes`) so non-UTF-8 bytes are
  returned, not a panic (the S-8d `property::<Option<String>>` form panicked on them).
- **verification**: UNVERIFIED — the GTK module is `cfg(linux|freebsd, feature = "gtk")` and was
  not compiled here (macOS, no VM for this batch). Needs the Linux lane (`--features
  gtk,x11,wayland` clippy).
- **state**: CLOSED (task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26): compiles and lints on Linux with `--features gtk,x11,wayland`; a scratch test under a private headless sway set `gtk-font-name` to `Mono \xff\xfe 11` through `g_object_set` and `get_alloc(GtkFontName)` returned those bytes (`Str([.., 255, 254, ..])`), `GtkXftDpi` gave `CInt(98304)` and `GtkEnablePrimaryPaste` `Bool(false)`; the S-8d form (`property::<Option<String>>`) panicked `Invalid UTF-8` on the same value.

## N283 — G3b had no audited way to mark a deliberately-not-carried comment

- **id**: N283. Not a ledger row: S-8b report concern 3.
- **severity**: low (gate). **kind**: gate gap.
- **description**: comments on code the port does not have by a recorded decision, and stripped
  gag prose, stayed MISSING permanently, so `--crate bin` / `--crate apprt` could never reach 0 and
  real drops hid among them. G3b now reads `.port/parity-closure/COMMENT-NOT-CARRIED.tsv`, whose
  rows are validated mechanically (`.port/tools/gates/_g3b_not_carried.py`): the text must be the
  oracle line at the pin; still MISSING/REVIEW in the port (a carried row is STALE, RED); for
  `no-counterpart` attached to the named reference code, with no Rust counterpart anywhere in the
  workspace and an unstruck evidence record naming it; for `gag-prose` free of digits, identifiers,
  backticks and ALL-CAPS tokens and at most 20 words; with a two-way cap. Excused comments are
  printed as `NOT-CARRIED (<class>)`.
- **S-8e (review CHANGES-REQUIRED, attacks A-1..A-3 went GREEN)**: hardened. `decl` must be DECLARED in
  the oracle file (fn/const/var, field, enum member) and not a Zig/Rust keyword or common local
  (`priv`, `self`, `alloc`, ...); a `///` doc or `PORTS:` comment citing it, or a Rust field of its
  name, is a counterpart (catches `BASELINE` = `comptime_version`); the evidence record (unique
  needle, at most 30 lines, a heading resolving to its own section only) must name the decl AND
  the oracle file and carry no strike/RESOLVED/PORTED ANYWHERE; a gag row's record must list its
  `file:line`, and its text may use no technical term or word of the surrounding oracle code unless
  `MIXED-CARRIED: "<port comment>"` names a comment the port carries. Rows the stricter rules would
  not admit, or that have a Rust site, are QUOTED there instead (port convention). 35 rows: bin 21
  (std_options_debug_io 6, DllMain 15), apprt 14 (adw_c 3, gtk_c 3, supportsDialogs 1,
  `Config.getMut` 3, gag-prose 4).
- **verification**: `check_comment_coverage.py --self-test` (69 ledger arms, synthetic and real,
  the review's A-1/A-2/A-3 planted verbatim); a mutation run (each W1-W4 rule weakened in-process)
  turns the matching arms FAIL; the review's NC-1..NC-4 and A-1..A-3 re-run on disk are all RED;
  `--not-carried-check` GREEN.
- **state**: CLOSED (task tail-s8d, hardened tail-s8e).

## N284 — two BlurRegion CSD-note lines stripped as gag prose carried a fact

- **id**: N284. Not a ledger row: found while classifying S-8b's stripped lines (task tail-s8d).
- **severity**: low (comment fidelity). **kind**: over-stripped comment.
- **oracle**: `src/apprt/gtk/winproto/BlurRegion.zig:59-61`.
- **description**: S-8b stripped `:59` ("... making a window ~30% bigger") and reworded `:61` as gag
  prose, but `:59` carries the fact the code acts on (CSD windows are ~30% bigger internally), and
  under the ledger's rule a line with technical content can never be gag prose (the self-test holds
  a real-row negative control for exactly this line). `:59-61` are restored verbatim in
  `crates/rustty-apprt/src/gtk/winproto/blur_region.rs`; only `:58` (an insult with no fact) is a
  `gag-prose` row.
- **state**: CLOSED (task tail-s8d).

## N285 — the FreeType arm of `test "face metrics"` has stale expectations (reference test-weakness)

- **id**: N285. Not a ledger row (task tail-s14, found running N265's live body under the newly
  selectable FreeType backends; settled by the S-14 review).
- **severity**: low (a reference test that never runs upstream — N265/UD276). **kind**: reference
  test-weakness (R13: stays faithful) — recorded as `UPSTREAM-DEFECTS.md` UD281.
- **oracle**: `src/font/Collection.zig:1402,1439-1502` at 20abdb50a — the FreeType arms
  (`.freetype`, `.fontconfig_freetype`, `.coretext_freetype`) expect, at 12 pt / 96 dpi, narrow
  (`embedded.cozette`) `cell_width` 8.0 / `ascii_height` 18.0625 and wide (`embedded.geist_mono`)
  `cell_width` 10.0 / `ascii_height` 16.0.
- **history (oracle git)**: `03a707b2c` (2025-09-17) wrote those values while the test ran;
  `4af4e1872` (next day, same PR) inverted its guard, so it has been skipped since (UD276);
  `ad9f9dc11` (2025-10-17, "font: Default to light hinting in FreeType") made `light = true` the
  default `FreetypeLoadFlags` (`FT_LOAD_TARGET_LIGHT`) and updated a RUNNING FreeType test, not this
  skipped one.
- **measurement** (port, `-Dfont-backend=coretext_freetype`; the S-14 review's probe, reverted):
  default flags (light) → narrow `cell_width` 7.0, `ascii_height` 18.0625; wide 10.0, 15.5.
  `light = false` → 8.0, 18.0625, 10.0, 16.0 — exactly the reference's expectations. The measuring
  code (`face/freetype.rs` `getGlyphSize`, the ASCII advance/bbox loop, `glyphLoadFlags`) is
  line-for-line the reference's at the pin, over the same FreeType 2.13.2. CORRECTION (S-14b): the
  first version of this entry said "the wide font (Geist Mono) matches"; its `ascii_height` does
  not (15.5 vs 16.0) — the assert loop panicked on the narrow pair first, so the wide pair had not
  been compared.
- **conclusion**: the expectations are stale; Ghostty at the pin, running its own test on a
  FreeType backend, would measure the same 7.0 / 15.5 (inferred from identical code and library,
  not run in Zig). Not a port bug.
- **state**: CLOSED (task tail-s14b). No code change; `face_metrics` stays a faithful skip.

## N286 — `-Dfont-backend`: which backends the port builds per target

- **id**: N286. Not a ledger row (task tail-s14; corrected by tail-s14b, review M3).
- **severity**: low. **kind**: R1 (Ghostty accepts every backend on every target); the one gap
  (Windows) is closed by task W-6.
- **oracle**: `src/build/Config.zig:191-195` validates nothing; `SharedDeps.zig:267-310` links
  HarfBuzz/FreeType/Fontconfig from source on any target whose backend has them. The reference's
  CI builds the per-OS defaults plus `coretext_freetype` on macOS (`test.yml:1383-1428`).
- **rustty (tail-s14b)**: `crates/rustty-build-config/build.rs` `font_backend()` accepts, on macOS,
  all seven native backends (`freetype`, `freetype_windows`, `fontconfig_freetype`, `coretext`,
  `coretext_freetype`, `coretext_harfbuzz`, `coretext_noshape`); on Linux/BSD `freetype`,
  `freetype_windows`, `fontconfig_freetype`; on Windows `freetype`, `freetype_windows`. It still
  refuses, loudly and by name:
  - the CoreText backends off Darwin — no CoreText there, in the reference too (justified);
  - `fontconfig_freetype` on Windows — **OPEN R1 gap**: the port gates `fontconfig-sys` off
    Windows (WS20) and has not built it there; not attempted (no Windows host here). (Superseded by
    task W-6 below: refused on MSVC only, as the reference fails there; built on GNU.)
  A native `web_canvas` has no feature at all (its face and shaper call browser externs;
  `font/main.zig:50` forces it on wasm), so it is not "refused by name" — it cannot be named.
- **correction**: tail-s14 refused `fontconfig_freetype` on macOS ("`fontconfig-sys` is gated off
  Darwin") and `freetype_windows` off Windows ("the directory scanner is Windows-only code"). Both
  reasons were wrong: `fontconfig-sys` builds on macOS (the `rustty-apprt` edge already built it
  there), and the scanner is portable Rust, as the reference's is portable Zig
  (`discovery.zig:1089-1115`). tail-s14b keys the `fc` module, `Descriptor.toFcPattern`, the
  Fontconfig tests and SharedGridSet's `@hasDecl(Discover, "discoverExactFamily")` on the backend,
  and allows both.
- **evidence (macOS, tail-s14b)**: the G-FONTBACKEND lane's `fontconfig_freetype` arm (a generated
  fonts.conf over the macOS font directories, since the vendored fontconfig looks in `/etc/fonts`
  like the reference's) passes font-shaper 142 (+3 ignored), including the `fc` package tests and the
  reference `fontconfig` / `fontconfig codepoint` tests; its `freetype_windows` arm (a `SYSTEMROOT`
  fixture whose `root\Fonts` holds Arial, Arial Unicode and Noto Sans Chakma) passes 119 (+3),
  including the reference `windows` test. Clippy `-D warnings` is clean for both.
- **task W-6 (2026-09-28), the reference measured at the pin** (Zig 0.16, the reference source at
  `20abdb50a` with its fetched `zig-pkg`): `Config.zig:100-111` gives a Windows target the MSVC ABI
  when none is named. (1) `zig build test -Dtarget=x86_64-windows -Demit-test-exe=true
  -Dfont-backend=fontconfig_freetype -Dtest-filter=fontconfig`, natively on `rwin` (Windows 11 ARM64,
  x86_64 Zig under emulation, VS 2022 Build Tools): FAILS, "compile lib fontconfig Debug
  x86_64-windows-msvc 56 errors", every one `src/fcint.h:44:10: error: 'unistd.h' file not found` (or
  `fccompat.c:44`): pkg/fontconfig/build.zig defines `HAVE_UNISTD_H` on every target and the MSVC
  headers have no `unistd.h`. Control, same host and Zig, default backend (`freetype_windows`),
  `-Dtest-filter=windows`: builds, "All 84 tests passed" (`font.discovery.test.windows` OK). (2) With
  an explicit GNU ABI, `-Dtarget=x86_64-windows-gnu` (cross-built on the Mac, MinGW headers from Zig):
  `zig build` installs `ghostty-internal.dll`, which carries fontconfig (`FONTCONFIG_FILE`,
  `WINDOWSFONTDIR` strings), and the filtered test exe run on `rwin` prints "All 76 tests passed",
  including `font.discovery.test.fontconfig`, `fontconfig codepoint` and
  `font.DeferredFace.test.fontconfig` (each after "Fontconfig error: Cannot load default config file:
  No such file: (null)", since no `fonts.conf` is installed). (The aarch64-windows Zig 0.16 build
  crashed with an access violation in its build runner on `rwin`, even on a `zig init` project; the
  x86_64 Zig was used.) So the reference accepts the option on Windows, cannot build it under its
  default (MSVC) ABI, and builds and runs it under the GNU ABI.
- **fix (W-6)**: match both halves. `rustty-build-config/build.rs` accepts `fontconfig_freetype` on
  `target_env = "gnu"` Windows and refuses it on MSVC by name, citing the reference's `unistd.h`
  failure and naming the GNU target. `rustty-font-shaper`'s `fontconfig-sys` table now excludes only
  MSVC Windows (and wasm). `libxml2-sys` leaves `LIBXML_ICONV_ENABLED` off on Windows, as
  pkg/fontconfig/build.zig:9-13 passes `.iconv = os != .windows` to libxml2 (before: the GNU build
  failed with `encoding.h:28: fatal error: iconv.h: No such file or directory`). `fontconfig-sys`
  adds the reference's Windows-only `-Werror=implicit-function-declaration` (build.zig:222-227).
- **evidence (W-6)**: `cargo test -p rustty-font-shaper --target x86_64-pc-windows-gnu --features
  rustty-build-config/font-backend-fontconfig-freetype --no-run` cross-built on the Mac (mingw-w64 gcc,
  `BINDGEN_EXTRA_CLANG_ARGS` pointing at its sysroot; the build-config output reads
  `font_backend=fontconfig_freetype`), the test exe run on `rwin` with the three MinGW runtime DLLs
  beside it: 131 passed, 0 failed, 4 ignored, three runs of three, including `fc::tests::*` (fc_list,
  fc_match, init/load-config), `discovery::tests::fontconfig`, `fontconfig_codepoint` and
  `deferred_face::tests::fontconfig`, with the same "Cannot load default config file" stderr as the
  reference. `cargo check -p rustty-build-config --target x86_64-pc-windows-msvc --features
  font-backend-fontconfig-freetype` panics with the new message; `freetype_windows` on MSVC and
  `fontconfig_freetype` on GNU pass it. Negative control: the old `cfg(not(any(target_os =
  "windows", ...)))` table restored alone, the GNU build fails with E0432, unresolved import
  `fontconfig_sys`. clippy `-D warnings` clean for the workspace (macOS) and for the GNU
  `fontconfig_freetype` font-shaper. UNVERIFIED: a native `x86_64-pc-windows-gnu` toolchain on
  Windows (none installed on `rwin`; the binaries were cross-built), and anything above
  rustty-font-shaper on Windows (rustty-core does not build there, N342).
- **state**: **CLOSED** (task W-6, 2026-09-28): parity. The reference accepts
  `-Dfont-backend=fontconfig_freetype` on Windows, fails to build it under its default MSVC ABI and
  builds it under the GNU ABI; the port now refuses it on MSVC with that reason and builds and tests
  it on GNU. Was: OPEN (triage T-1) for `fontconfig_freetype` on Windows only.

## N287 — the `freetype_windows` `DeferredFace` name accessors did not type-check

- **id**: N287. Not a ledger row (task tail-s14; the lines predate it).
- **severity**: medium (every `x86_64-pc-windows-*` build of `rustty-font-shaper` failed).
  **kind**: port defect (compile error on the Windows default backend).
- **rustty**: `crates/rustty-font-shaper/src/deferred_face.rs` `windows_payload` `family_name` /
  `name` returned `w.peek.name(buf)` — which may borrow the FreeType face — from a `&self` method
  whose result was tied only to `buf`: E0521-class "lifetime may not live long enough".
- **fix**: both take `&'a self`, as the CoreText payload's do. Found and verified by
  `cargo check --tests --target x86_64-pc-windows-gnu` (zig cc, mingw headers), default and
  `-Dfont-backend=freetype`. Not run on Windows.
- **state**: CLOSED (task tail-s14).

## N288 — `loadCoreTextFreetype` opens face 0 of a font collection (reference defect)

- **id**: N288. Not a ledger row (task tail-s14).
- **severity**: medium. **kind**: reference defect, non-crash, evident intent broken (the user's
  R13 extension) — `UPSTREAM-DEFECTS.md` UD280.
- **oracle**: `src/font/DeferredFace.zig:301-304` at 20abdb50a: "Face index 0 is not always
  correct. We don't ship this configuration in a release build." then `Face.initFile(lib, path, 0,
  opts)`.
- **evidence**: CoreText's `Menlo-Bold` lives in `/System/Library/Fonts/Menlo.ttc`; with index 0
  the FreeType face loaded is `Menlo-Regular` (measured, macOS 26, test below before the fix).
- **rustty**: `deferred_face.rs` `collection_face_index` picks the face whose PostScript name is
  the CoreText font's (index 0 when the file holds one face or none matches). Test
  `load_coretext_freetype_opens_the_discovered_face_of_a_collection`; negative control = the
  reference's index 0 ("Menlo-Bold" != "Menlo-Regular").
- **state**: CLOSED (task tail-s14), recorded as UD280.
## N290 — G-SKIP hardened against the S-3 review's eight bypasses (S-3b)

- **id**: N290. Not a ledger row: the S-3 review's non-blocking G-SKIP findings (task S-15, item A).
- **severity**: medium (gate). **kind**: gate gap.
- **what**: `check_silent_skips.py` accepted these silent skips: (1) any earlier assertion in the
  fn body excused a later top-level `return` (and a trivially-true or `debug_assert!` one excused
  a guarded return); (2) a `match` with an empty `None =>`/`Err(_) =>` arm; (3) an `else` that only
  prints, without the word "skip" (and an if-let wrapper beside a trivial assertion); (4) a
  `return` inside a closure / nested fn that is the whole test; (5) any fn NAMED
  `require_<x>_or_skip` counted as a pivot; (6) every assertion inside a `for` / `.map(..)` over a
  possibly-empty probe result; (7) `#[rstest]` / `#[test_case]` tests were not scanned; (8) the lint
  walked the filesystem (untracked files changed the verdict) and treated any `*test*` basename as
  test code.
- **fix**: (1) the excusing assertion must sit in the guard's own block (the statement for a
  top-level return, the arm for an unbraced match arm), at its top nesting level, and be
  non-trivial (`debug_assert*` and assertions naming no identifier but `true`/`false`, or comparing
  identical operands, never count); (2) new ARM rule; (3) WRAP now covers any `if` whose assertions
  leave a silent path (no `else`, or an `else` without assertion/pivot), with trivial outside
  assertions discounted; (4) bare returns in closures / nested fns are checked when an assertion
  follows them in that body; (5) a `require_<x>_or_skip` must transitively (same file) both read
  a `"RUSTTY_REQUIRE_<X>"` literal and fail, else a PIVOT finding, and pivot / pivoting-helper
  acceptance requires that pair; (6) new LOOP rule (literal tables exempt); (7) `TEST_ATTR` covers
  `#[rstest]` and `#[test_case(..)]`; (8) `git ls-files` (a non-git root is an ERROR, exit 1), and
  test-only files are the `tests/` tree plus files loaded by `cfg(test)`-scoped `mod` declarations
  (`#[path]` resolved, transitively; `cfg(any(test, ..))` included).
- **self-test**: 59 arms (was 21), each RED arm checked for its exact finding kind; per bypass a
  planted RED arm and a clean arm, plus the review's missing accept arms (MSG after a pivot, WRAP
  with a scoped ignore, `ALLOW_GUARDS` over its cap). Mutation run (each new rule weakened
  in-process, scratch `s15/mutate.py`): every weakening turns its arms FAIL (B1 ×3 mutations, B2,
  B3, B4, B5, B6, B7, B8 ×2).
- **real tree**: two silent skips found and fixed (not allow-listed):
  `rustty-global` `environ_from_process_matches_vars_os` (LOOP: now counts the round-tripped keys and
  asserts at least one) and `rustty-xev` `kqueue_file_io_on_thread_pool` (RET: returned `ok`
  having asserted only the deferral ledger, inside a conditional; now `#[ignore]`d and FAILS when run,
  naming the unported macOS `xev.File` watcher, with the ledger tripwire split into the always-run
  `kqueue_file_watcher_deferral_is_ledgered`). No allow-list change (4/6 sites, 2/2 guards).
  Test-only file set vs the old basename rule: `rustty-oniguruma/src/testing.rs` (production) is no
  longer test code; 26 declared test modules not named `*test*` (the `log_capture.rs` files,
  `lz4_differential.rs`, `snapshot/fixture.rs`, ...) now are.
- **state**: CLOSED (task S-15).

## N291 — the display-link test is enforced on the macOS leg; stale "Noto Serif Tibetan" comments

- **id**: N291. Not a ledger row: the S-3 review's follow-ups 2 and 4 (task S-15, item A).
- **what / fix**: `.github/workflows/ci.yml` now exports `RUSTTY_REQUIRE_DISPLAY=1` on the macOS leg
  (env step beside METAL / SYSTEM_FONTS), so `cargo test --workspace` in `ci.sh` runs
  `sync_parks_the_link_while_idle_and_runs_it_while_dirty` as enforced evidence; `PORT_RULES.md`
  R14's table says so. Local check: `RUSTTY_REQUIRE_DISPLAY=1 RUSTTY_REQUIRE_METAL=1 cargo test -p
  rustty-renderer --lib sync_parks_the_link` passes (the link exists). UNVERIFIED: whether the hosted
  macOS image has an active display; if the leg then fails with "active display required but
  unavailable", that is the finding. The `ci.yml`, `coretext_tests.rs` and `harfbuzz_tests.rs`
  comments said Noto Serif Tibetan is an `#[ignore]`d test "on every leg" / among the macOS tests;
  it is only the HarfBuzz `shape_tibetan_characters`, which is not compiled on macOS and is
  `#[ignore]`d elsewhere. Comments corrected (comment-only edits in `rustty-font-shaper`).
- **state**: CLOSED (task S-15).

## N292 — theme lookup `BufferTooSmall` → `OutOfMemory`, and the C API's `err=` form (N140 closed)

- **id**: N292. Not a ledger row: the closure of NEW-FINDINGS N140 (a) and (b) (task S-15, item B).
- **severity**: low. **kind**: behavioral-difference (error identity and propagation; log text).
- **what / fix / tests / negative controls**: recorded under N140 ("CLOSED by task S-15").
- **files**: `crates/rustty-config/src/{theme.rs,config_struct.rs,zig_error.rs,c_api.rs,lib.rs}`,
  `crates/rustty-config/tests/load_path_errors_propagate.rs`,
  `crates/rustty-cli/src/{rustty.rs,zig_error.rs,list_themes.rs}`.
- **state**: CLOSED (task S-15).

## N293 — `readthread_pipeline_shuts_down_cleanly_from_both_stages` bounded a live drain with a wall clock

- **id**: N293. Not a ledger row: the flaky test queued by S-6 (task S-15, item C).
- **severity**: low (flaky test). **kind**: test defect (port-only KAT; the reference has no
  equivalent test — `src/termio/Exec.zig` at `20abdb50a` has only the `execCommand` tests).
- **measured**: at base the test takes 5.5-6.0 s. About 3 s is the eight blocking 64 KiB writes
  to the slave waiting on the parse stage, and 2.2-2.8 s is the drain AFTER the quit: 145-170 KiB
  were still in flight (ring plus pty queue) and the parse stage parses them before it exits, at
  about 65 KiB/s in the `dev` profile. `sample(1)` of the parse stage: the top frames are
  `rustty_vt::page::mask`, `page::get_bits` and `Page::verify_integrity`, i.e. the reference's
  `assertIntegrity` under `slow_runtime_safety` (`page.zig:366-376`, Debug only;
  `SLOW_RUNTIME_SAFETY = cfg!(debug_assertions)` in the port).
- **root cause**: not a port slowdown. The drain-on-quit is the reference's design (parse loop
  returns on `done` only once `count == 0`; the gather stage `continue`s on a full buffer without
  polling `quit`), and the port matches it line for line. The test bounded that finite, live drain
  with a fixed 10 s `recv_timeout`; under load the drain crosses 10 s (S-6: load 20-29, 1 run in 8;
  here, 42 spinning processes on 14 cores: drain 5.8-9.96 s, one batch gap 4.1-4.7 s).
- **fix**: the watchdog now fails only when the read thread has not joined AND no batch has reached
  `processOutput` for 30 s (the regression it guards — a stage exiting without waking the other —
  leaves the parse stage asleep on `batch_ready`, so progress stops for good). 30 s is over 6x the
  loaded per-batch gap. The test also now asserts its premise (bytes still in flight at the quit)
  and each bulk write's length, and prints `SHUTDOWN-DRAIN` with the drain time and longest gap.
- **verification**: 3/3 idle (drain 2.2-2.8 s, gap 0.73-0.79 s), 3/3 under the synthetic load.
  Negative control (gather's `DoneSignal` no longer sets `done`): FAILED after 30.0 s with "no
  batch reached processOutput for 30s", reverted.
- **state**: CLOSED (task S-15).

## N295 — the S74-F3 cwd-error test ran into `loadCliArgs`'s Linux fast path

- **id**: N295. Not a ledger row: found by task L-1 running `cargo test --workspace` on Linux.
- **severity**: low. **kind**: test portability (port-only test).
- **what**: `load_cli_args_returns_the_cwd_realpath_error` (rustty-config) failed on Linux with
  `got Ok(())`: on Linux and FreeBSD `loadCliArgs` returns at once when argv has one element
  (`Config.zig`: `.linux, .freebsd => if (std.os.argv.len <= 1) return`, ported), and
  `cargo test --workspace` starts the test binary with no arguments. Faithful behaviour, wrong test.
- **fix**: on Linux/FreeBSD the test re-runs itself in a child whose argv holds the libtest filter,
  asserts the child's argv is longer than one element and that the child ran exactly this test
  (`ac13a16e`). Linux: passes; macOS: passes.
- **state**: CLOSED (task L-1).

## N296 — the paste-event mutex test ticked its event loop once

- **id**: N296. Not a ledger row: found by task L-1 on Linux.
- **severity**: low. **kind**: test portability (port-only test; LEDGER S42-F1's test).
- **what**: `paste_event_queues_while_holding_the_renderer_mutex_on_a_full_mailbox` (rustty-core)
  failed deterministically on Linux: `the mailbox never signalled its wakeup (timed out after 10 s:
  false)`, after 2 ms. Measured with a probe: libxev's epoll backend counts the timer's submission as
  a completion, so `run(Once)` returned after about 12 us with neither the wakeup nor the timer
  callback run (a second tick delivered the wakeup). kqueue on macOS happened to wait.
- **fix**: tick until one of the two callbacks has fired; the 10 s timer still bounds it (`be7f5237`).
  Linux and macOS pass.
- **state**: CLOSED (task L-1).

## N297 — `io_thread` crashed on Linux: an `input` failure after the shell starts leaves the loop on a dead frame (UD290)

- **id**: N297. Not a ledger row: found by task L-1 (the `io_thread` test binary died with SIGSEGV on
  Linux in `oversized_input_file_paints_the_input_failure_screen`, LEDGER S212-F1's test).
- **severity**: medium. **kind**: reference defect (use-after-free and leak), fixed under R13.
- **what / fix / proof**: `UPSTREAM-DEFECTS.md` UD290; `crates/rustty-termio/src/termio_impl.rs`
  `Termio::threadEnter` (PORT NOTE: reference defect (R13)). Found with gdb: the fault is in the epoll
  loop's submission-queue pop during `Thread::threadMain`'s drain run.
- **state**: CLOSED (task L-1).

## N298 — two Linux test-portability fixes: the CMake install directory and the gather-stage hangup fixture

- **id**: N298. Not a ledger row: found by task L-1 on Linux.
- **severity**: low. **kind**: test portability (port-only tests).
- **what**: (1) `install_layout_is_versioned_like_the_reference` (xtask, LEDGER S64-F5) looked in
  `prefix/lib`; GNUInstallDirs installs into `lib64` on Fedora, where the layout itself was right
  (`librustty-vt.so.0.1.0`, `.so.0`, `.so`, SONAME `librustty-vt.so.0`). (2)
  `gather_stage_reads_output_that_arrives_with_the_hangup` (rustty-termio, UD19) made its hangup with
  `shutdown(SHUT_WR)`; Linux reports `POLLHUP` on a Unix stream socket only when both directions are
  shut, so the gather stage saw `POLLIN`, read 0 forever and the test timed out (measured with a C
  probe: `SHUT_WR` and `SHUT_RDWR` both give `0x11` on macOS).
- **fix**: (1) the test finds `lib` or `lib64` and now also asserts the ELF SONAME with `readelf`
  (`297a28fc`); (2) `shutdown(SHUT_RDWR)`. Both pass on Linux and macOS; (2)'s negative control on
  Linux (the reference's decision) still fails `left: [102, 105, 114, 115, 116]`.
- **state**: CLOSED (task L-1).

## N299 — the reference's Highway CPU detection is inert on Linux (UD291)

- **id**: N299. Not a ledger row: found by task L-1 verifying LEDGER S34-F3 against the reference.
- **severity**: low. **kind**: reference defect (performance; evident intent), port already correct.
- **what**: on the VM the reference's `ghostty_hwy_detect_targets` (zig 0.16.0, aarch64-linux, `-lc`)
  returns `0x20000000` while the same code without libc, and the port, return `0x34000000`. Zig's
  `std.os.linux.getauxval` reads an auxv pointer only Zig's own `_start` sets. Details and proposed
  fix: `UPSTREAM-DEFECTS.md` UD291; PORT NOTE at the port's detector (`d03d273d`).
- **state**: CLOSED (task L-1).


## N300 — the port aborted on every surface drag-and-drop, and the headed KAT that emits `drop` had stopped reaching it (UD295)

- **id**: N300. Not a ledger row: found by task L-2 running the `--ignored` headed GTK integration tests
  under a private headless sway on the VM.
- **severity**: high for the port (a process abort on a user gesture). **kind**: port defect, from a
  reference defect (UD295).
- **what**: `surface_drop` (`crates/rustty-apprt/src/gtk/class/surface.rs`) was a `()` template
  callback on `GtkDropTarget::drop`, a `gboolean` signal, faithful to the reference's
  `callconv(.c) void` `surfaceDrop`. glib-rs panics on the missing return value inside the C callback,
  which aborts the process; so dragging a split onto another surface killed Rustty. It now returns
  `bool` (UD295, R13). Test `surface_drop_answers_the_gboolean_drop_signal`.
- **how it hid**: the only test that emits `drop`, the headed KAT `gtk_parity_callbacks`, had stopped
  reaching it: since task 20 (S54-F3) typed `mouse-shape` it panicked earlier on
  `set_property("mouse-shape", 2i32)`, and its `drop` emission passed a bare string where the signal
  declares a boxed `GValue`. Both are fixed; it now emits `drop` on every drop target of the surface.
  No CI lane runs the headed KATs (`#[ignore]`d; R14 form 2 "no lane has the resource").
- **not stale**: `gtk_application::application_new_then_deinit` and
  `gtk_first_light::first_light_renders_a_live_terminal_frame` assert that a "noop-only build"
  selects the `.none` windowing protocol, so they fail under `--features gtk,x11,wayland`; built with
  `--features gtk` as their `#[ignore]` reasons say, both pass on the VM (task L-2).
- **state**: CLOSED (task L-2).

## N310 — G-LOGFILE allowances driven from 131 over 50 files to 17 over 8, each now a port or a machine-checked row

- **id**: N310. Not a ledger row: the S-1c batch (follow-up of N190 and task tail-s1b), branch
  `tail-s1c` = tail-s1b merged with parity-closure `a3e92e1d`.
- **severity**: low. **kind**: gate accounting + R1 restorations (missing diagnostics).
- **before / after**: after the merge the portmap carried 131 allowance units over 50 reference files
  (tail-s1b alone: 137 over 53; parity-closure's T-A/T-F/T-G had already restored 6). Now 17 over 8,
  each row's reason names its sites and the open finding that holds it (N312, N313, N314, and
  `stream_terminal.zig`'s 7, which batch S7-4 restores on its own branch).
- **ported live** (the reference logs; the port had the call as a comment, most under a stale "no
  logging facade in this crate" note — `rustty-vt` has logged through `log` since parity S156-F4),
  each with a log-capture test and a negative control run one at a time (the `log` call disabled
  with `#[cfg(any())]`; all 33 FAILED for the stated reason, restored): `osc/parsers/kitty_color.zig`
  :35 :42 :46 :66; `context_signal.zig` :219 :234 :245 :256; `kitty_desktop_notification.zig` :299 :312
  :322; `hyperlink.zig` :43; `iterm2.zig` :192 (Zig `{t}` = the key's tag name); `kitty/dnd_drop.zig`
  :31; `kitty/graphics_unicode.zig` :447 :453 :459 :468; `kitty/graphics_exec.zig` :35 :39 :60 :125
  (:304 `track_pin` and :420 `index` fail only on allocation: ported, not driven, UNVERIFIED by test);
  `kitty/graphics_command.zig` :281; `kitty/clipboard_write.zig` :151 :236; `apc/glyph/execute.zig` :35;
  `dcs.zig` :33 :37 :117 (:56 is behind `!tmux_control_mode`, off in this build: ported, not driven);
  `Terminal.zig` :1437 :4837 :4857 (the alt-screen cursor copy failure is driven through the existing
  `CURSOR_COPY_TW` tripwire for modes 47, 1047 and 1049); `page.zig` :372 (logged before the panic).
- **ported live, not testable on macOS** (compile verified only where noted): `KeymapDarwin.zig:117`
  (`log::debug!` with the empty target of an unscoped `std.log`; `rustty-input` gained its `log`
  dependency; compiled on macOS, not driven: needs a TIS input source); GTK `application.zig` :1640 :1644
  :1825 :1874 :1994 and :2197 (`gtk.rs` `register_resources`, which printed `rustty: unable to load
  resources` outside the facade), and :3024's text restored (`, out of memory` was lost) — Linux-pending.
- **text restored to the reference** (R13, message text stays faithful): `application.zig` :2462 :2603
  :3276 say `ignoring new_tab` in `gotoTab`/`moveTab`/`toggleTabOverview`; the port had "corrected" them
  to `goto_tab`/`move_tab`/`toggle_tab_overview`. Recorded as UD300. Linux-pending.
- **renamed so the gate counts it**: the CoreText and FreeType variation-axes dumps (face/coretext.zig
  :124 :151, face/freetype.zig :122 :130) were live since S90-F2/S91-F8, but in a helper named
  `log_variation_axes`, which G-LOGFILE reads as a one-message `log_*` facade; now `dump_variation_axes`.
  `font/Atlas.zig:466` printed `warning: ...` (not the logFn shape) and now prints `warn(atlas): ...`;
  rustty-font has no `log` facade (D1), so the file's bypass cap is 2.
- **declared, machine-checked**: 59 `LOG-ABSENT-SITES.tsv` rows (catch arms of calls the port cannot
  fail, or states its types cannot hold: config C API and wasm, Config.zig, DeferredFace, shaper
  coretext, image.zig on both twins, metal buffer, opengl Buffer.zig, Surface.zig, PageCompression,
  inspector renderer/termio, stream_handler, search Thread, osc color and kitty_color's appends, and
  in GTK command_palette, media, wayland and application); 3 `LOG-DEAD-SITES.tsv` rows
  (`bitmap_allocator.zig:215` in the uncalled private `dumpBitmaps`; `Termio.zig:705`, whose
  `recordPtyRead` has an EMPTY inferred error set, so the reference's own catch cannot run;
  `Program.zig:120` in `setUniform`, which nothing names); 2 `facade` rows (resize_overlay's and
  split_tree's `remove_source_id`); 5 `LOG-PORT-ONLY.tsv` rows (N311 x3, N312's reclassified copy,
  and PageList `compact`, below).
- **gate change** (`check_log_reconcile.py`, self-tested; every arm below fails under its own mutation,
  9 mutations each alone, file restored and hash-checked): (1) the test-only rule now spans the TREE:
  a `pub fn` every tree-wide reference of which is in test code (fixed point, with its private
  callees) is test-only — this drops `lib/enum.zig` :138 :154 (`checkGhosttyHEnum`, called only from
  `test` blocks) and `PageList.zig:3674` (`compact` has no caller outside tests at the pin), and names
  81 such `pub fn`s in the report; references are matched by NAME (conservative), a quoted name keeps a
  fn counted, only same-name re-exports are transparent; (2) `LOG-DEAD-SITES.tsv` kind
  `unreferenced-pub`, verified against the same index; (3) `LOG-PORT-ONLY.tsv` kind
  `test-only-reference` (the port counterpart of a site the tree rule drops, verified: the match must be
  a test-only site's message) — used for PageList `compact`, whose port compiles and counts the
  emission.
- **update (task S-1d, review S-1c)**:
  - **F1**: the tree rule classified a name test-only when its only references were inside its own
    same-name bodies (delegation), i.e. when it had no caller at all (`redrawInspector`, four defs, no
    test reference, was reported test-only; no site moved). A name now needs at least one caller
    outside its bodies; with none it stays counted. Self-test arm (delegation fixture) and mutation.
  - **F2**: `pub fn`s called by comptime declaration reflection (`font/sprite/Face.zig:60-104` walks
    the `draw*` fns of eight imported files) were invisible to the name index. Every `pub fn` of a
    file `@import`ed by a file with a production `@typeInfo(T).@"struct".decls` /
    `std.meta.declarations` line is now reflected: never test-only, never `unreferenced-pub`. 38 files
    at the pin; the report now names 78 test-only fns, and the same 3 sites drop.
  - **F5**: the gate's logFn prefix accepted `warn(`/`err(`, which no logFn prints, and rejected
    `warning(`, the text Zig's `Level.asText()` gives. It now accepts exactly `error|warning|info|
    debug` with `(<scope>): ` or, for the default scope, `: `. 25 direct prints that carried the wrong
    prefix are corrected (`warn(` -> `warning(`, `err(` -> `error(`): imgui_widget.rs x6,
    inspector_widget.rs x1, post_fork.rs x7, cgroup.rs x1, search/thread.rs x4, search/terminal.rs x5,
    atlas.rs x1. Atlas.zig's two sites go through `log` now (rustty-font gains `log`; the S-1c "D1"
    reason was a misreading: D1 forbids a vt edge), bypass cap 2 -> 0. tripwire.rs's correct
    `warning(tripwire)` print is counted for the first time: bypass cap 1. The controller may assign
    this record a separate id (the N310-N314 block had none left).
  - **merge**: at the S-1d merge of parity-closure `602e8f2b` the absent rows media.zig:45 (T-D) and
    c/terminal.zig :475 :527 :538 (S7-4) were deleted (those sites are live), and N312's row (below).
  - Allowances: 17 over 8 files at tail-s1c -> 3 over 2 (N314) after the merge, F4 and N313.
- **state**: CLOSED (task S-1c; updated by task S-1d). The remainder is N311 and N314.

## N311 — the GTK apprt logs three errors the reference propagates with `try`

- **id**: N311. Not a ledger row: found by task S-1c adjudicating `application.zig`.
- **severity**: low. **kind**: behavioral-difference (extra diagnostics; possible double logging).
- **oracle**: `src/apprt/gtk/class/application.zig:2443-2482` `reloadConfig` `try`s `Config.load`,
  `core_surface.updateConfig` and `self.core().updateConfig`, with no log.
- **rustty**: `crates/rustty-apprt/src/gtk/class/application.rs` `reload_config_inner` logs
  `error loading config on reload err=` and `error updating surface config err=`, and
  `update_config_app` logs `error updating app config err=`, where each flattens the error into the
  unit `ApprtError` and returns it.
- **open question**: whether a caller of the port also logs the (now valueless) error, which would
  make these duplicates, or whether they are the only record of the error value, which the unit
  `ApprtError` would otherwise lose. UNVERIFIED either way (GTK code; not traced on Linux).
- **gate**: declared `open-finding` in `LOG-PORT-ONLY.tsv` (delete the rows in the commit that
  resolves this).
- **state**: **CLOSED** (task G-2, 2026-09-28). `action::reload_config` now returns a local `ReloadConfigError` (`Load(LoadError)` from `CoreConfig.load`, `Core(app::Error)` from the app or surface `updateConfig`) whose Display is the Zig name (`rustty_config::zig_error::load_error` for load errors; `error.OutOfMemory` for the allocating core errors `updateConfig` can return, the core Display otherwise, which N11 owns). The three inner logs are gone (`update_config_app` returns the core error; `reload_config_inner` maps with `?`), and the three `open-finding` rows are deleted from `LOG-PORT-ONLY.tsv` (cap 57 to 54). The callers log once with the name: `handle_reload_config` and `handle_sigusr2` `"error reloading config: {err}"` (`application.zig:1602-1606`, `:1682-1690`), and `action_reload_config` now prints the pin's own text `"error reloading config err={err}"` (`application.zig:1792-1794`; it carried the other callers' text `"error reloading config: "` and Debug before). The `performAction` arm converts to the unit `ApprtError` (`From<ReloadConfigError>`), the apprt seam's error type; a keybind-driven reload that fails therefore still reaches the core's caller line valueless, a property of the unit `ApprtError` seam shared by every apprt action, not of this site. Tests: `reload_config_error_prints_the_zig_error_name` (pure) and `reload_config_failure_logs_once_at_the_caller_with_the_error_name` (display; `rvm`, broadway): the bound app `updateConfig` thunk fails with the core's OutOfMemory; `reload_config` returns `error.OutOfMemory` and logs nothing, and the `app.reload-config` handler logs exactly `(Warn, "gtk_rustty_application", "error reloading config err=error.OutOfMemory")`. Negative controls on `rvm`, one at a time: re-adding the inner `update_config_app` log fails "reloadConfig propagates its error with `try` and logs nothing" (captured the inner record); the caller printing `{err:?}` fails with `err=Core(Surface(InitChangeConfig(InitChangeConfigError)))`. G-LOGFILE GREEN (54 port-only rows). Was: **OPEN** (triage T-1, 2026-09-28). Still present (`application.rs:664`, `:4979`, `:5006`) and answerable now: the reference logs once at the caller (`log.warn("error reloading config: {}", .{err})`, `application.zig:1602-1606`, which the port also has at `application.rs:1164`, `:1828`, `:2421`), so the port logs twice, and the caller's line prints the valueless `ApprtError` where the reference prints the error name. Left: carry the error (or its Zig name) through `ApprtError`, drop the three inner logs, delete the three `open-finding` rows in `LOG-PORT-ONLY.tsv`. Effort S-M, no long run, GTK so it needs a Linux build on `rvm`; observable in log text (one extra line, one valueless one), R1. Was: OPEN.

## N312 — `load_custom_css` reports a read failure as an open failure and keeps going

- **id**: N312. Not a ledger row: found by task S-1c adjudicating `application.zig:1310`.
- **severity**: low. **kind**: missing-branch (R1).
- **oracle**: `src/apprt/gtk/class/application.zig:1140-1180` `loadCustomCss`: `openFileAbsolute`
  fails -> `error opening gtk-custom-css file {s}: {}` (unless an optional file is missing) and
  `continue`; `readToEndAlloc` fails with `FileTooBig` -> the size warning and `continue`; any OTHER
  read error -> `return e`, which `propConfig` logs as :1310 `failed to load custom CSS, no custom CSS
  applied, err={}`, and the remaining paths are not loaded.
- **rustty**: `load_custom_css` does `std::fs::metadata` then `std::fs::read`, and logs a failed
  `read` with the OPEN message (:1154's text) and continues, so :1310 is never emitted and the later
  paths are still loaded.
- **fix (not applied: GTK code, cannot compile on macOS)**: open with `File::open` (open failure =
  :1154 arm), read with `read_to_end` bounded by the 5 MiB limit (`FileTooBig` arm), and return any
  other read error from `load_custom_css`; `prop_config` logs it with :1310's text. Then delete the
  `load_custom_css#1` `open-finding` row and set application.zig's allowance to 0.
- **gate**: application.zig allowance 1 names this site.
- **update (task S-1d, the merge of parity-closure `602e8f2b`)**: parity tail T-D (S49-F3) had already
  made this change on parity-closure: `load_custom_css` returns the read error, `prop_config` logs :1310
  (application.rs `prop_config`), and only one `error opening gtk-custom-css file` copy is left. After the
  merge the `load_custom_css#1` `open-finding` row would have selected that genuine :1154 counterpart and
  subtracted it (review S-1c F4), so the row is deleted (port-only cap 57 -> 56) and application.zig's
  allowance is 0.
- **state**: CLOSED (superseded by T-D S49-F3; records reconciled at the S-1d merge).

## N313 — the Zig build graph's own diagnostics have no counterpart in the cargo/xtask build

- **id**: N313. Not a ledger row: found by task S-1c.
- **severity**: low. **kind**: gate accounting (decision needed).
- **sites**: `src/build/Config.zig` :135 (iOS rejected for the full build) :445 (`-Dvt-features`
  names an unknown feature); `src/build/GhosttyDist.zig:245` (a dist resource in a git checkout);
  `src/build/combine_archives.zig` :19 :52 and `src/build/wasm_patch_growable_table.zig:23` (helper
  binaries' usage and exit errors).
- **port**: the build is cargo + `rustty-build-config` + xtask. The two Config.zig checks and the
  dist-resource check do not exist there (UNVERIFIED for GhosttyDist's `Resource.exists`); the port
  has no `combine_archives` binary (libtool/lipo in `archives.rs`); xtask's `wasm-patch-growable-table`
  reports its usage error as a returned `Err` string printed by xtask's `main`, not as a logFn line.
- **correction (review S-1c F7)**: this entry said a `build-tooling:` exclusion "stops a future
  upstream site in them from turning the gate RED". That is false: `#!exclusion-cap files=N sites=M`
  is an exact two-way ratchet on the excluded files' site count, so a new site in an excluded file is
  RED, as it is under an allowance.
- **resolution (task S-1d, controller decision on the reviewer's per-site plan)**:
  - `Config.zig:135` PORTED: the `rustty` app crate's build.rs `reject_unsupported_target` prints
    the default-scope line `error: iOS is not a supported target for the full Rustty build; only
    librustty-vt supports iOS (-Demit-lib-vt)` (R2 rename declared `identity-rename` in
    LOG-TEXT-DIVERGENCES.tsv) and exits 1. `build_checks::full_build_rejects_ios_only` tests the
    predicate; `cargo check -p rustty --target aarch64-apple-ios` measured failing with the line.
  - `Config.zig:445`: LOG-ABSENT-SITES `unrepresentable-state` (cargo rejects an unknown feature
    itself: "the package 'rustty-vt-c' does not contain this feature", measured).
  - `GhosttyDist.zig:245`: the port never chooses between a dist and a generated resource (rustty-cli
    and rustty-apprt build scripts always generate; xtask's dist ships none), so LOG-ABSENT-SITES
    `unrepresentable-state`.
  - `combine_archives.zig` :19 :52: `build-tooling:` EXCLUSION (the binary does not exist in the
    port); exclusion budget 2/2 -> 3/4.
  - `wasm_patch_growable_table.zig:23` PORTED: xtask's `wasm-patch-growable-table` usage error prints
    `error: usage: wasm_growable_table <input.wasm> <output.wasm>` (`report_usage_error`) and exits 1
    (measured), keeping the reference's name, a latent copy-paste (UD301). The tests now assert the
    `WasmPatchError::Usage` variant.
  - All six build/* allowance units are 0.
- **state**: CLOSED (task S-1d).

## N314 — two reference sites are test-only or never compiled, but the gate cannot verify it

- **id**: N314. Not a ledger row: found by task S-1c.
- **severity**: low. **kind**: gate limit.
- **`src/terminal/snapshot/fixture.zig:227`**: in `pub fn expectEqual`, which every caller reaches
  from a `test` block as `test_fixture.expectEqual` (the port's fixture.rs is `#[cfg(test)]`). The
  tree-wide test-only rule (N310) matches references by NAME, and a different `expectEqual` is written
  in production code (`src/datastruct/comparison.zig:155-186`), so the rule keeps the site counted, as
  designed. Closing it needs receiver-aware resolution (only `.expectEqual` after an alias of
  fixture.zig counts).
- **`src/unicode/grapheme.zig:221 :233`**: in `pub fn main`, a debug entry point no build step
  compiles (its doc comment says so). `unreferenced-pub` cannot verify it because `main` is written all
  over the tree. The port's `main` prints its progress with a plain `eprintln!` and cannot port the
  uucode comparison (R3; the external parity harness does that).
- **gate**: fixture.zig allowance 1, grapheme.zig allowance 2, each naming this finding.
- **state**: **STANDING (measurement caveat, not a defect)** (triage T-1, 2026-09-28): the three sites are hand-verified as test-only (`fixture.zig:227`) or never compiled (`grapheme.zig:221`, `:233`), so the reference emits none of them in a product build; they are G-LOGFILE's only remaining allowance units (fixture.zig 1, grapheme.zig 2), each naming this entry. Making the gate prove it (receiver-aware resolution) is optional gate precision, no port change is owed. Was: OPEN.

## N315 — S7-5 routing: the render state's contents, through the allocator it is updated with

- **id**: N315. Not a ledger row: records the S7-5 scope decisions (closes S175-F1; S4-F2's render half).
- **severity**: informational. **kind**: allocator routing (T1/T2/T3 of the S-7 design).
- **routed (T1)**: every container of `rustty_vt::render::RenderState` grows through the allocator
  `update`/`beginUpdate` is given and frees through it: `row_data`, each row's three cell columns
  (`raw`, grapheme spans, `style`), its grapheme buffer, `highlights` and `applied_styles`, and
  `pending_styles`. vt-c `new_` resolves `lib.alloc.default(alloc_)` to the runtime handle and keeps it
  as `RenderStateWrapper.alloc` (the wrapper's Rust-heap `Box<dyn ZigAllocator>` holder is gone), and
  `update`/`begin_update`/`free` pass `state.alloc` (`c/render.zig:209-232`). Nothing reaches the
  terminal's allocator or the Rust heap unless that is the state's allocator (the application's
  `Global`). Not routed (T1/T2 residual outside the C API): `string` and `link_cells` ignore the
  `alloc` they are passed (render.rs `string`'s `_alloc`, `link_cells`' `let _ = alloc`): the writer,
  the `StringMap` (`Vec<point::Coordinate>`) and the `CellSet` (zigstd `AutoArrayHashMapUnmanaged`
  over `Global`, whose `put` is infallible) stay on the Rust heap and abort on OOM, where the
  reference's `try m.map.appendNTimes(m.alloc, ..)`/`append` and `try result.put(alloc, ..)` return
  `error.OutOfMemory`. Every caller passes the Rust heap (the renderer's link and regex paths), and the
  C API does not expose either.
- **fallibility (T2)**: each reference `try` is reachable and returns `error.OutOfMemory`
  (`GHOSTTY_OUT_OF_MEMORY` through the C API): `row_data.resize`, `cells.resize`, the three
  `pending_styles.append`s, the grapheme `dupe`, `applied_styles.ensureTotalCapacity`, and the
  highlight `append` in `updateHighlightsFlattened`. A failed update leaves the state memory-safe
  (lengths only change after their growth succeeds; unflagged/stale grapheme spans are read through a
  checked slice). The retry after a failure: UD315 / N316.
- **per-call vs bound**: the reference passes the allocator per call; the lists bind to it on first
  growth (`alloc::bind`, rows are created with it) and later calls debug-assert it
  (`render_state_checks_the_update_allocator`).
- **USER-ACCEPTED, C-observable call-shape difference (cell storage; review S7-5 F2, accepted by the
  user 2026-09-26)**: a row's cells are three allocations (the `raw`, grapheme-span and `style`
  columns), not the reference's single `MultiArrayList` block. A C caller's allocator sees three
  alloc (and later three free) calls per row growth instead of one (a 24-row first update: 73
  allocations where the reference makes 25), per-column sizes and alignments instead of
  `capacityInBytes(cap)` at `big_align`, and therefore different failure indices under a
  failing-after-n allocator; a failure on the second or third column adds an alloc/free pair the
  reference never makes (the state is left unchanged). Reason: the faithful single block was
  implemented and MEASURED at +3.4% cycles on `screen-clone.render` (1.8% fewer instructions; the
  growCapacity headroom between its columns spreads the hot `raw`/`style` columns), outside the +2%
  fence; the per-column lists measured +0.1%. S7-9 carries this in the T3 record.
- **with overscan (P7-W3 T1, `ca4f719f7`)**: overscan multiplies the rows built, and so the three-for-one
  call count, by the rows captured (`rowDataRange()`, at most `above + rows + below`); `row_data`
  itself is still one allocation, and entries outside the captured range are never built in either
  implementation. MEASURED (CountingAllocator, 80x24, 100 lines of history, first update): no
  request, 24 rows built, 73 allocations (reference 25); a request of `{above 4, below 1}` (the
  ScreenClone bench's) scrolled 10 rows into history, 29 rows built, 88 allocations (reference 30);
  the same request following the active area (nothing below), 28 rows built, 85 allocations
  (reference 29). In general `1 + 3n` against `1 + n` for `n` captured plain-text rows.
- **T3 (call shape)**: the MultiArrayLists grow as Zig 0.16's `resize` does (to `growCapacity(n)`
  elements, alloc + copy + free, never `remap`); the `ArrayList`s to `growCapacity(n)` by `remap`
  first, then alloc + copy + free. The `init_capacity` of each is computed from the REFERENCE's sizes
  (`Row` fields 24 bytes, `Cell` fields 28, `StyleRun` 36, `Highlight` 6, on a 128-byte cache line),
  so the element counts the caller's allocator sees match the reference's; byte sizes differ where the
  Rust element does. Residuals: (1) a row's cells are one allocation per column, not one for all
  three: MEASURED (`time -l`, paired, interleaved, load 7-15), the single-allocation layout cost
  +3.4% cycles on `screen-clone.render` at 1.8% fewer instructions (the growCapacity headroom between
  its columns spread the hot `raw`/`style` columns of a viewport's rows), and the per-column lists
  measured +0.1%; each column still grows by `setCapacity`'s alloc + copy + free, all three allocated
  before any is replaced. (2) The per-row arena (grapheme dupes, highlights) is a retained grapheme
  buffer and a retained highlight list on the same allocator, cleared with capacity kept where the
  reference resets the arena (`.retain_capacity`) or sets `.empty`: a real arena would hand out
  grapheme slices that dangle for the stale slots a safe API exposes. The steady state matches (no
  allocation once capacities are reached); the growth sequence differs. (3) The grapheme buffer grows
  by `try_reserve` (amortized doubling), an arena stand-in.
- **perf**: the out-of-line `try_append`/`ensure_total_capacity` calls first measured ~4% of the
  render samples; they are inline now with the growth out of line. The fence is in the S7-5 report.
- **state**: CLOSED (task S7-5).

## N316 — a failed render-state update is followed by a partial rebuild (UD315)

- **id**: N316. Not a ledger row: found by task S7-5 when the render state's allocations became
  fallible (before S7-5 they aborted, so the path was unreachable in the port).
- **severity**: low. **kind**: reference defect (non-crash, evident intent).
- **what**: `beginUpdate` stores the new dimensions and viewport pin and clears page and row dirty
  flags before the allocations that can fail; after a failure the next update is incremental and
  leaves the unreached rows stale or empty. Details, measurement and proposed fix:
  `UPSTREAM-DEFECTS.md` UD315.
- **fix / proof**: `RenderState::begin_update` records the failure and the next update redraws
  (PORT NOTE: reference defect (R13, UD315)); `failed_update_is_followed_by_a_full_rebuild` and both
  allocation-failure sweeps compare each retried update with a fresh state; the negative control (the
  reference's behaviour) fails them.
- **state**: CLOSED (task S7-5).

## N320 — S7-7 routing and borrowing: the OSC parser (S174-F3 closed; N263 resolved)

- **id**: N320. Not a ledger row: records the S7-7 scope decisions (closes S174-F3).
- **severity**: informational. **kind**: allocator routing (T1/T2/T3 of the S-7 design) and borrowed payloads.
- **routed (T1)**: everything the reference sends through `parser.alloc` at the pin (`src/terminal/osc.zig`,
  `osc/parsers/*.zig`, `c/osc.zig`): the C parser itself (`alloc.create(osc.Parser)`, vt-c `create`), the
  allocating capture (part 1, tail-a2), `color_operation.requests` (`SegmentedList(Request, 2)`: nothing for up to
  two requests, the shelf array and shelf 0 for the third, then a new shelf array and shelf 1 for the seventh) and
  `kitty_color_protocol.list` (an `ArrayList`, first reservation `growCapacity(1)`). Nothing else: the reference's
  other parsers allocate nothing, and neither does the port now.
- **N263 resolved**: the command's text payloads were owned `Vec<u8>` copies (about 15 `Command` variants, the
  nested kitty clipboard / dnd / desktop-notification / text-sizing / semantic-prompt / context-signal payloads,
  their option values, and a deep `Box<Command>` clone per dispatch through `Parser::next` and a clone on the stream's
  OSC fast path). They are borrowed now: `osc::Command<'p>` holds `ZStr<'p>` (`[:0]const u8`: the NUL sentinel is
  inside the borrow, so a C reader of `rustty_osc_command_data`'s string stays inside it) and `&'p [u8]`. The lists
  are the parser's (`color_requests`, `kitty_color`: what the reference's `parser.command` owns) and are freed at
  `reset`/`deinit` as the reference's switch frees them; the command borrows them, owns nothing, and is `Copy` like
  the reference's `cmd.*`.
- **lifetime design**: `Parser::end(&mut self) -> Option<Command<'_>>`. The reference's documented lifetime ("only
  valid until the next call to the parser") is the borrow, checked by the compiler; no raw pointers. The command is
  not a parser field (it would be self-referential and dangle when the parser moves). `parser::Action<'p>` carries
  the borrowed command; `Parser::next` computes the `osc_string` exit last (unobservable: the table leaves
  `osc_string` only on BEL/ESC/CAN/SUB, whose transitions and entries do not touch the OSC parser); the stream
  detaches the other actions (`Action::into_static`) and dispatches `osc_dispatch` against the handler alone. The one
  lifetime erasure is in vt-c: the wrapper keeps the last command for the C pointer and clears it first in every
  entry point that touches the parser (`next`, `reset`, `end`, `free`); the wrapper never moves, and the header's
  lifetime ("valid until the next call ... except command introspection") is exactly that. No header change.
- **fallibility (T2)**: `parseColor` failing takes the reference's `catch` (`log.info("failed to parse OSC {t} color
  request err={} data={s}")`, an empty list); a refused kitty `append` warns and continues; the capture keeps its
  fixed fallback and `WriteFailed` → invalid; `rustty_osc_new` answers `OUT_OF_MEMORY`. The C-API failure sweep
  (`osc_allocation_failure_sweep_through_the_c_api`) swept 14 allocation indices and every remap index (2).
- **T3 / deviations**: the kitty list's element size is Rust's, so `growCapacity(1)`'s `cache_line / @sizeOf(T)`
  differs from Zig's; SegmentedList `shrinkCapacity` always takes the "resize refused" arm (allocator_api2 has no
  in-place resize; the global heap's adapter refuses anyway, N273); the per-dispatch `Box` of the borrowed command in
  `Parser::next` stays on the Rust heap (the existing PERF boxing of every dispatch payload, `CSI`/`ESC`/`DCS` too;
  the reference copies the union by value). Test adaptations: reference tests that read `p.state` while holding
  `Parser::next`'s actions use a detached-actions helper; `osc: change window title (end in esc)` reads the command
  before `next('\\')`, which the borrow requires (the reference reads it after, relying on that call not touching
  the OSC buffer).
- **API changes** (Rust-side, no C change): `osc::Parser` and `vt::Parser` lose their lifetime parameter;
  `stream::Stream<H>` (was `Stream<'a, H>`); `stream::Options.allocator: Option<alloc::Allocator>`;
  `Stream::init_in` removed (S7-4's); `Handler::semantic_prompt(SemanticPrompt<'_>)`,
  `kitty_clipboard(KittyClipboardProtocol<'_>)`, `kitty_dnd(KittyDndProtocol<'_>)`,
  `kitty_color_report(&kitty::color::OSC)`, `color_operation(ColorOperation<'_>)` (with `requests: &List`).
- **state**: CLOSED (parity tail S7-7).

## N321 — `parseGetSetDynamicColor` leaks its grown list when a later `addOne` fails (reference defect, R13; UD310)

- **id**: N321. Not a ledger row: found by S7-7 porting the list's growth.
- **severity**: low. **kind**: reference defect (leak on allocation failure), fixed under R13.
- **where**: `src/terminal/osc/parsers/color.zig` `parseGetSetDynamicColor` at pin `20abdb50a`: `var result: List =
  .{};` with no `errdefer result.deinit(alloc);`, unlike its three siblings (`parseGetSetAnsiColor`,
  `parseResetAnsiColor`, `parseResetDynamicColor`).
- **reachable**: an OSC 10, 11, 12 or 13 from the pty with seven or more values (S7-7b, review F2: each value
  advances with `Dynamic.next()`, which ends at 19, so OSC 14 takes at most six and OSC 15-19 fewer), while the
  parser's allocator refuses
  the seventh request's growth (a C embedder's limited allocator): the third request grew the shelf array and
  shelf 0, the seventh's `try result.addOne(alloc)` fails, and `result` is dropped without `deinit`. ReleaseSafe and
  ReleaseFast: the two allocations leak (the caller's `catch` returns an empty list).
- **port**: `color::List` frees on drop, so the error path frees what it grew (`PORT NOTE: reference defect (R13),
  N321` in `crates/rustty-vt/src/osc/parsers/color.rs`). Test `dynamic_color_list_growth_failure_frees_what_it_grew`
  (FailingAllocator `fail_index 2`: allocated == freed, and the catch's log ran); negative control: the list
  forgotten on the error path leaks 28 bytes and fails the test.
- **upstream fix**: add `errdefer result.deinit(alloc);` after `var result: List = .{};`.
- **UD id**: UD310 (S7-7b), with its `docs/ghostty-bug-fixes.md` row.
- **state**: CLOSED (parity tail S7-7).

## N322 — the vendored `SegmentedList.growCapacity` cleanup frees the slot that failed (latent reference defect; UD311)

- **id**: N322. Not a ledger row: found by S7-7 porting `src/datastruct/segmented_list.zig`.
- **severity**: low. **kind**: latent reference defect (vendored Zig 0.15.2 std code).
- **where**: `growCapacity`'s `errdefer while (i > old_shelf_count) : (i -= 1) { allocator.free(
  new_dynamic_segments[i][0..shelfSize(i)]); }`: when the allocation of shelf `i` fails, the cleanup frees
  `new_dynamic_segments[i]` (never written: an undefined pointer) and stops before `old_shelf_count` (leaking the
  first new shelf). ReleaseSafe: the slot is `undefined` (0xaa… bytes), so an invalid free (the general-purpose
  allocator panics; a C allocator's behaviour is the embedder's); ReleaseFast: undefined behaviour; both leak the
  first new shelf (S7-7b, review F3).
- **reachable**: only when one call adds two or more shelves (`setCapacity`/`growCapacity` with a large capacity)
  and a later shelf fails. Every caller in the reference only appends, one shelf per call: the OSC color list
  (`addOne`) and src/font/Collection.zig:698 (`SegmentedList(EntryOrAlias, 0)`), so it is latent across the whole
  reference.
- **port**: frees exactly the shelves the call allocated (`PORT NOTE ... N322` in `rustty_datastruct::segmented_list`);
  test `a_refused_shelf_in_a_multi_shelf_growth_frees_the_new_shelves` (exact call log); negative control: without
  the cleanup the shelves leak and the call log differs.
- **upstream fix**: free indices `old_shelf_count..i` (e.g. `errdefer while (i > old_shelf_count) { i -= 1;
  allocator.free(...[i]...); }`).
- **UD id**: UD311 (latent; no `docs/ghostty-bug-fixes.md` row).
- **state**: CLOSED (parity tail S7-7).

## N323 — `ghostty_osc_command_data` traps on a NULL command the header allows (reference defect, R13; UD312)

- **id**: N323. Not a ledger row: found by S7-7's C-API failure sweep.
- **severity**: medium. **kind**: reference defect (crash / UB), fixed under R13.
- **where**: `src/terminal/c/osc.zig` `commandDataTyped`: `const command = command_.?;`. The header
  (`include/ghostty/vt/osc.h`) documents `command` as "may be NULL", and `ghostty_osc_end` returns NULL for every
  invalid or unfinished OSC, although the same header documents `end` as never returning NULL (S7-7b, review F3).
  Minimal trigger: `ghostty_osc_end` on a parser fed nothing, then `command_data` for the title.
- **reachable**: a C caller that asks the result of `end` for its title (`GHOSTTY_OSC_DATA_CHANGE_WINDOW_TITLE_STR`)
  after an OSC the parser rejected (bytes from the pty). ReleaseSafe: panic; ReleaseFast: a NULL dereference (UB).
  The port had the same dereference (`&*command_`).
- **port**: a NULL command answers `false` and leaves `out` untouched, as `commandType` answers NULL with `.invalid`
  (`PORT NOTE: reference defect (R13), N323` in `crates/rustty-vt-c/src/osc.rs`). Test
  `command_data_on_a_null_command_is_false`; negative control: without the guard the test process aborts on the
  NULL dereference.
- **upstream fix**: `const command = command_ orelse return false;` (or make `end` honour its header).
- **UD id**: UD312 (S7-7b), with its `docs/ghostty-bug-fixes.md` row. S7-7b also recorded UD313 (review F4): a
  second `ghostty_osc_end` on the same color/kitty-color OSC leaks the first list in the reference; the port frees it
  (test `end_twice_frees_the_first_list`). No N id (none left in S7-7's block).
- **state**: CLOSED (parity tail S7-7).

## N324 — a borrowed OSC command that owned its lists cost the per-byte parser loop up to 2.2x

- **id**: N324. Not a ledger row: found by S7-7's perf fence.
- **severity**: informational. **kind**: performance (codegen), resolved.
- **what**: with the borrowed `Command<'p>` owning its two lists, `terminal-parser` measured +43% (utf8) and +118%
  (pure ASCII) against `d8bb504d`. The per-byte `[Option<Action>; 3]` that `Parser::next` returns is discarded, and
  it costs nothing only while LLVM folds its drop away after inlining. The lists' frees (through the allocator
  handle, may-unwind to the caller's codegen) put unwind cleanup into the array's drop, which then indexed the
  array dynamically and kept it in memory, and the larger drop glue stopped the jump threading that makes the print
  path skip it. Outlining the exit, `nounwind` `extern "C"` frees and an out-of-line command drop each recovered
  part and moved the register allocation of the rest.
- **fix**: the lists belong to the parser (as the reference's `parser.command` owns them) and are freed at `reset`;
  the command owns nothing, and `Box<Command>`'s drop is a deallocation like the other boxed actions. Paired fence
  (N=31, load 6-11): osc-parser -4.5%, terminal-parser/utf8 -0.6%, terminal-parser/ascii -0.5%,
  terminal-stream/ascii -0.3%, an OSC-heavy stream -6.6%.
- **lesson**: anything that gives `Action`'s drop glue work or an unwind edge shows on the per-byte parser loop.
  Note that `utf8.bin` is OSC-heavy for the VT parser: its many `0x9D` bytes (a UTF-8 continuation byte) enter
  `osc_string`.
- **state**: CLOSED (parity tail S7-7).

## N325 — S7-6 routing: the search's state through the search's allocator, four allocations through the terminal's (S176-F1, S4-F2 closed)

- **id**: N325. Not a ledger row: records the S7-6 scope decisions (closes S176-F1 and, with S7-5's render
  half, S4-F2).
- **severity**: informational. **kind**: allocator routing (T1/T2/T3 of the S-7 design).
- **routed to the search's allocator (T1)**: everything `TerminalSearch.init(alloc, ..)` and the objects under it
  allocate, re-verified at the pin in `search/{terminal,screen,viewport,active,pagelist,sliding_window}.zig` and
  `highlight.zig`: the sliding windows' needle (`dupe`) and overlap buffer, their data and meta circular buffers
  (`CircBuf.init(alloc, 0)`, `resize` = `realloc` to exactly `len + amount`), the chunk buffer
  (`MultiArrayList.ensureTotalCapacity`), each page's `cell_map` (grown by the page formatter's `PointMap`), the
  temporary encoding buffer (`std.Io.Writer.Allocating`), and the decode buffer of a compressed page
  (`node.pagePreservingState(self.alloc)`); the screen searches' `active_results`/`history_results` (and the
  reload's rebuilt list, `shrinkAndFree` on pruning) and every stored result's chunks (`Flattened.clone(alloc)`);
  the viewport fingerprint (`ensureTotalCapacity(alloc, rows)`); the terminal search's cached viewport matches.
  `ScreenSearch.matches(alloc)` allocates its slice through the allocator it is GIVEN. vt-c: the wrapper
  (`alloc.create`) and `TerminalSearch.init(wrapper.alloc, ..)` use `lib.alloc.default(alloc_)`, kept as the
  runtime handle (the wrapper's Rust-heap `Box<dyn ZigAllocator>` holder is gone).
- **charged to the TERMINAL's allocator (N262, reproduced)**: the vt-c `searches` registry entry
  (`putNoClobber(terminal.gpa(), ..)`, bound by S7-4) and the tracked pins: the PageListSearch pin, the history
  start pin, and the selected match's two pins (`Tracked.init`), all through the screen's `PageList.trackPin`
  (its pin pool and pin set are the terminal's, S7-3). Tests count them on the terminal's pin set and show the
  search's allocator holds nothing else.
- **fallibility (T2)**: every reference `try` in the search is reachable and returns `error.OutOfMemory`; the
  formatter's point-map growth returns `error.WriteFailed` as the reference's `catch return error.WriteFailed`
  (N328). OOM propagates or is logged exactly where the reference logs it. A C-API feed/run that cannot progress
  reports OUT_OF_MEMORY (UD305, N327).
- **per-call vs bound**: the reference's unmanaged lists take the allocator per call; the port's lists bind to the
  search's allocator at creation (the fingerprint on first growth, `alloc::bind`), and `deinit(alloc)` of a
  `Flattened`, a `Meta`, a circular buffer or the fingerprint debug-asserts the allocator
  (`flattened_deinit_checks_the_allocator`).
- **T3 (call shape)**: growth follows Zig 0.16 (`zig_list.rs`): `ArrayList` to `growCapacity(n)` by `remap` first;
  `MultiArrayList` to `growCapacity(n)` by alloc + copy + free (never `remap`); `CircBuf.resize` = `realloc` (remap
  first) to exactly the requested size; `Writer.Allocating` drains with `ensureUnusedCapacity(2 * len + 1)`.
  `init_capacity` uses the REFERENCE's element sizes, so element counts match. Residuals: (1) byte sizes differ
  where the Rust element does: a chunk is 24 bytes in an array of structs against 20 in Zig's struct of arrays
  (a clone of a one-chunk highlight is 17 x 24 bytes, not 17 x 20, at 8-byte alignment); a stored `Flattened` is
  40 bytes against 32; `matches()`'s slice holds 24-byte shallow copies against 32. (2) The encoding buffer's
  calls follow the port formatter's write granularity (its 512-byte cell-run buffer mirrors the reference's; not
  verified call for call). (3) The window's private `CircBuf` asserts growth only (the window never shrinks; the
  shared `rustty_datastruct::CircBuf` ports shrinking). (4) The meta buffer's slots are `Option<Meta>` (48 bytes,
  the same as Zig's `Meta`), the `undefined` default being `None`.
- **not routed**: the search thread (`search/thread.rs`, the application's) builds its search on the Rust heap
  handle, as the reference's thread uses the application allocator. The formatter's `PinMap` points stay on the
  Rust heap (N328).
- **state**: CLOSED (task S7-6).

## N326 — port-only search copies removed; FLATTENED_TW and REGISTER_TW retired

- **id**: N326. Not a ledger row: found by task S7-6 routing the search.
- **severity**: informational. **kind**: port-only allocations (the N263 class) and port-only seams.
- **what**: the port made Rust-heap copies the reference never makes, on every call: `SlidingWindow.next` cloned
  the chunk buffer into each returned highlight (the reference's aliases it, "valid until the next call");
  `matchAt`/`selectedMatch` and `selectNext`/`selectPrev` cloned a stored result (the reference copies the value
  shallowly); `matches` deep-cloned every result; the terminal and screen searches copied the needle
  (`to_vec`) before `ScreenSearch.init`/`PageListSearch.init`/`SlidingWindow.init`. They were infallible (an abort
  under OOM) and invisible to the caller's allocator.
- **fix**: `highlight::Flattened<C>` holds either owned chunks (the default, cloned through a given allocator) or
  borrowed ones (`FlattenedRef<'a>`, the reference's shallow copy); `next()`, `matchAt`, `selectedMatch`,
  `matches` and the search thread's `selected_match` event return borrows, and the borrow checker enforces the
  reference's lifetime. The needle borrows end before the searches are replaced, so no copy is needed.
- **seams retired (N271/N277 class)**: `highlight::FLATTENED_TW` (Flattened init/clone failure) and vt-c
  `search::REGISTER_TW` (registry growth failure) are replaced by `FailingAllocator`s: under the flattened
  highlight's allocator and under the TERMINAL (the registry is terminal-charged). The adapted
  `tests/viewport_fingerprint_alloc.rs` stays (it checks the Rust heap handle); the reference's
  `fingerprint only allocates on changes` is now ported as written in `search/viewport.rs`.
- **state**: CLOSED (task S7-6).

## N327 — the C API's search feed and run spin under allocation failure (reference defect, R13; UD305)

- **id**: N327. Not a ledger row: found by task S7-6's C-API failure sweep design (a sticky failing allocator
  makes `run` loop).
- **severity**: medium. **kind**: reference defect (hang).
- **what**: `TerminalSearch.feed` swallows allocation failures, so a search waiting for its next history page stays
  `feed_required`; `ghostty_search_run` loops forever and `ghostty_search_feed` keeps answering SUCCESS. Details,
  measurement and proposed fix: `UPSTREAM-DEFECTS.md` UD305.
- **fix / proof**: vt-c `feed` and `run` return OUT_OF_MEMORY when a feed leaves the search `feed_required`
  (PORT NOTE: reference defect (R13, UD305)); `search_feed_and_run_report_out_of_memory_instead_of_spinning` and
  the two C-API sweeps; negative controls in UD305.
- **state**: CLOSED (task S7-6).

## N328 — the page formatter's point map grows fallibly through its allocator; the PinMap's points stay on the Rust heap

- **id**: N328. Not a ledger row: found by task S7-6 (the sliding window's `cell_map` is written by
  `PageFormatter`'s `PointMap`).
- **severity**: informational. **kind**: allocator routing / fallibility at a shared type.
- **what**: `PointMap.map` was a Rust-heap `Vec` with infallible pushes, where the reference appends through
  `map.alloc` with `catch return error.WriteFailed` (formatter.zig:1123-1276, 1446-1571, 1749, 1854, 2063).
- **fix**: `PointMap.map` is an allocator-bound list; every append site grows as `ArrayList.appendNTimes`
  (`growCapacity`, remap first) and maps a failure to an `io::ErrorKind::OutOfMemory` write error (the
  reference's `WriteFailed`), which the window reports as `error.OutOfMemory`. The appends fill inline and grow
  out of line (PERF(port), measured).
- **residual (formatter family, outside S7-6)**: `PinMapMap.points` shares the type and is bound to the Rust
  heap handle; its `nodes` list, the pin map's `alloc`, and the formatter's other lists are unchanged (the
  file-wide convention: Rust-heap growth that aborts on OOM). The `alloc` fields of `PinMap` are not asserted
  against the points' allocator for that reason.
- **state**: **CLOSED** (triage T-1, 2026-09-28): the search part closed by S7-6, the formatter residual by task S7-8 (N330, state CLOSED; `5bf7e3e5`). Was: CLOSED for the search. The formatter residual is OPEN, owned by S7-8
  (`DEFERRALS.md` F-008/W12-01 Tier 2 item 5). The window's own `PointMap` construction
  debug-asserts that the list is on the window's allocator (S7-6b). The formatter residual is CLOSED by task
  S7-8 (N330).

## N329 — Stacked Borrows status of the search after S7-6

- **id**: N329. Not a ledger row: the SB check task S7-6 was asked to make.
- **severity**: informational. **kind**: soundness (UNVERIFIED by Miri here; Miri not run by the task).
- **the 2026-07-11 finding**: the search SB UB (search objects holding raw `Screen` aliases invalidated by
  `&mut Screen`; memory `search-screen-sb-ub-finding`) was FIXED by `9e3fe1c` (raw `*mut Screen` in
  `Untracked::track`, `Tracked::init`/`deinit`, `SelectedMatch`, `HistorySearch::deinit`, `ref_counted_set::get`).
  Re-checked on this branch: every one of those signatures still takes `*mut Screen`, and `reload_active_inner`
  still derives `list` with `addr_of_mut!((*self.screen).pages)`. S7-6 adds no `&mut Screen`/`&mut PageList`
  over a searched screen.
- **new aliasing shapes to prove under Miri**: (1) `FlattenedRef<'_>` borrows from `SlidingWindow.next()` held
  across a mutation of a SIBLING field (`self.active.next()` then `self.active_results` growth; `history.searcher`
  vs `self.history_results`); (2) the window's private `CircBuf::get_ptr_slice` (two raw subslices from one
  `split_at_mut`) read in `next()` while `highlight(&mut self)` runs after the last use; (3) `shrink_and_free`'s
  raw-parts round trip; (4) `AllocatingWriter::write`'s raw copy into spare capacity; (5) `append_n_times`'s raw
  fill. Harness: `.port/phase-2/mirihar-s7-6-search` (isolated, non-FFI, reproduces 1-5 with the same borrow
  shapes; for the controller's Miri lane under `-Zmiri-stacked-borrows` and `-Zmiri-tree-borrows`).
- **state**: **CLOSED (verified)** (G-1, 2026-09-28). The isolated harness `.port/phase-2/mirihar-s7-6-search` (current tree: S7-6b's `without_provenance_mut` history node) run per its README, each candidate separately, under Stacked Borrows (default) and Tree Borrows (`MIRIFLAGS=-Zmiri-tree-borrows`), `CARGO_TARGET_DIR` outside the tree: `window_next`, `tick_sibling`, `list_helpers` each printed `ok <name>` and exited 0 under both models, no Miri warning printed (miri 0.1.0 `6f72b5dd5f` 2026-07-22, rustc 1.99.0-nightly; about 2 s in all, well under the 20-min cap). Together with C-1's green `rustty-vt` G6 lane (SB) this closes the verification. Was: **OPEN (verification only)** (triage T-1, 2026-09-28). Partial evidence since: task C-1's full `rustty-vt` G6 Miri lane (default Stacked Borrows, `26453ffb`, 2026-09-27, a tree that contains S7-6 `67896ff8`) is green, lib 3015 passed. Not done: the isolated harness `.port/phase-2/mirihar-s7-6-search` has no recorded run under `-Zmiri-stacked-borrows` or `-Zmiri-tree-borrows`. Left: run it under both. Effort S (an isolated non-FFI harness, minutes, not a long run, but it is Miri, so the controller's lane); internal soundness, no observable parity effect. Was: OPEN for the controller's Miri lane (targets listed in the S7-6 report).

## N330 — the formatter pin map grows through the pin map's allocator (the N328 residual)

- **id**: N330. Not a ledger row: closes the formatter residual of N328 (review S7-6 F3, `DEFERRALS.md`
  F-008 Tier 2 item 5). Task S7-8.
- **severity**: low. **kind**: allocator routing (T1/T2/T3 of the S-7 design).
- **oracle**: `src/terminal/formatter.zig:129-231` (`PinMap.Map`: `points`/`nodes` are `std.ArrayList`s grown
  by `setNode(alloc, ..)`/`append(alloc, ..)` with `appendNTimes`), `:388-540`, `:824-840` (the extras' `m.map.append(m.alloc, ..)
  catch return error.WriteFailed`), `:916-923` (`PageListFormatter`: `m.map.setNode(m.alloc, chunk.node) catch
  return error.WriteFailed` and `.point_map = .{ .alloc = m.alloc, .map = &m.map.points, .base = .. }`);
  `Screen.zig:2944-2995` (`selectionStringMap` passes its `alloc` as the pin map's); at 20abdb50a.
- **rustty (before)**: `PinMapMap.points` was an allocator-bound list bound to the Rust heap handle, `nodes` a
  std `Vec`, `set_node`/`append` infallible (`resize_copy`, `push`), and `PointMap` grew the list it points to
  without reading its `alloc`.
- **fix**: `crates/rustty-vt/src/formatter.rs` `PinMapMap`: both lists are `AVec`s bound to the pin map's
  `alloc` on first growth (`alloc::bind`, identity-asserted afterwards and in `deinit`), grown with Zig's
  `ArrayList` growth (`zig_list::append`/`append_n_times`, `init_capacity` 16 for `Coordinate`, 8 for the
  16-byte `NodeRun`); `set_node`/`append` return `OutOfMemory`, which every caller maps to the reference's
  `error.WriteFailed` (an `io::ErrorKind::OutOfMemory` write error). `PageListFormatter` binds `points` before
  handing it to the page formatter; `PointMap`'s growth debug-asserts its `alloc` is the list's.
  `StringMap::deinit` frees the map through `PinMapMap::deinit(alloc)`.
- **verification**: `formatter::tests::pin_map_plain_routes_through_the_pin_maps_allocator` (exact: 2 allocations,
  `nodes` capacity 9, `points` capacity 17, 144 + 136 live bytes, both freed by `deinit`),
  `pin_map_extras_route_through_the_pin_maps_allocator` (palette extras first: `points` capacity
  growCapacity(5522) = 8299), `pin_map_growth_failure_is_write_failed` (plain and vt, first and second growth
  refused), `selection_string_map_routes_through_the_given_allocator`. Negative controls, one at a time:
  `PageListFormatter`'s bind removed (3 tests fail: the identity assert), `set_node`'s bind removed (4 fail:
  `nodes` on Global), `append`'s bind removed (the extras test fails: identity assert), `append` grown with the
  node-run `init_capacity` (8291 vs 8299), the palette extras' `map_err(..)?` replaced by `expect` (the failure
  test panics).
- **residual**: `StringMap::shallow_copy` (oniguruma search matches) deep-copies the map; the copy is on the
  map's own allocator (the reference's copy is shallow and allocates nothing). Every caller of
  `selection_string_map` passes the Rust heap handle and it is not reachable from the C API. See N334.
- **state**: CLOSED (task S7-8). N328's residual is closed by this entry.

## N331 — the C API's allocating writers grow through the reference's allocator and hand over their buffer (`toOwnedSlice`)

- **id**: N331. Not a ledger row: found by the S7-8 audit of what F-008 Tier 2 still left on the Rust heap.
- **severity**: medium (C API allocator contract). **kind**: allocator routing, T1/T2/T3.
- **oracle**: `c/formatter.zig:220-238` and `c/selection.zig:226-249` (`format_alloc`: `var aw:
  std.Io.Writer.Allocating = .init(lib.alloc.default(alloc_))`, `aw.toOwnedSlice() catch return
  .out_of_memory`), `c/snapshot.zig:622-656` (`encode_alloc`, the same over the caller's allocator),
  `c/terminal.zig:975-1001` (`continuationAllocIo(.., alloc, ..)`: an allocating writer over `alloc`,
  `toOwnedSlice`; `alloc.dupe(u8, "")` for an untracked ground stream), `:1084-1114` (`continuation_alloc`
  passes `lib.alloc.default(alloc_)`), `:759` (`restoreContinuation`: `Writer.Allocating.init(
  wrapper.terminal.gpa())`), `c/snapshot.zig:512-542` (`continuationOwned`: the copy on `native.gpa()`); Zig 0.16
  `std/array_list.zig` `toOwnedSlice` (`remap` to the length, else alloc + copy + free); at 20abdb50a.
- **rustty (before)**: each built its output in a Rust-heap `Vec` (infallible growth) and, for the four C
  exports, copied it into one exact allocation from the caller's allocator. So the caller's allocator saw one
  call where the reference makes its growth calls, a Rust-heap allocation it never sees held the whole output
  (and aborted instead of `OUT_OF_MEMORY`), and `restoreContinuation`/`continuationOwned` never touched the
  terminal's allocator. `rustty_selection_format_alloc` also made a RAW `alloc(0)` callback for empty output
  (the S171-F1 class; a NULL answer was papered over with a dangling pointer).
- **fix**: `zig_list::AllocatingWriter` (S7-6's port of `Writer.Allocating`, now public as
  `alloc::AllocatingWriter`) gains `to_owned_slice` (`ArrayList.toOwnedSlice`: no call for a writer that never
  allocated; otherwise the handle's `shrink` = `remap` then alloc + copy + free; on failure the writer keeps and
  later frees its buffer) and implements the snapshot `io::Writer`. The five sites use it over the reference's
  allocator; `continuation_alloc_io` takes the allocator again and returns an `AVec` bound to it;
  `allocator::into_c_bytes` hands the exactly-sized buffer across the ABI (the non-null empty sentinel for 0
  bytes, as before).
- **T3 residuals**: the formatter's write granularity follows the port formatter's (N325). The continuation
  tracker's writes were a one-byte `write` (`ensureUnusedCapacity(1 + 1 + 1)` = 132 bytes) where the
  reference's `writeByte` rebases (`growCapacity(1)` = 129); S7-8b (review F4) made `Tracker::write` and
  `Stream::write_continuation` take a `stream_continuation::ByteWriter` (a `std::io::Write` with `writeByte`,
  overridden by `alloc::AllocatingWriter`), so the first growth is 129 as in the reference. Negative control
  (the override removed): `continuation_writers_route_through_their_allocators` fails, 129 vs 132.
- **NULL allocator**: `format_alloc_null_allocator_round_trip` (S7-8b, review F3) produces through the Global
  arm and frees with `rustty_free(NULL, ..)`, text and empty output.
- **verification** (rustty-vt-c unless noted): `format_alloc_writer_routes_through_the_callers_allocator` and
  `selection_format_alloc_writer_routes_…` (1 allocation of growCapacity(11) = 144 bytes, 1 remap to 5, no
  free), `encode_alloc_writer_routes_…` (caller: 1 allocation of 153, 2 remaps, 1095 bytes handed over;
  terminal: the continuation copy and the record scratch, N332), `continuation_writers_route_through_their_allocators`
  (`continuation_alloc`: 129 then a remap to 4; `restoreContinuation`: exactly one 129-byte allocation and free
  on the terminal's allocator, and `OutOfMemory` when it is refused), the failure arms of each (first growth
  refused; remap refused and the copy refused), `allocation_failure_sweep_through_the_allocating_writers`
  (alloc loop, remap loop, and an alloc loop with every remap refused, which reaches the `toOwnedSlice`
  fallback), rustty-vt `zig_list::tests::allocating_writer_to_owned_slice_is_zigs`. The reference's snapshot
  test's `FailingAllocator(.fail_index = 0)` now runs as written (it was a rejecting `CountingCAllocator`).
  Negative controls, one at a time: each of the five writers and the snapshot's continuation copy on the Rust
  heap (each fails its routing test: `left: 1, right: 0` or the exact count), `toOwnedSlice` without the shrink
  (every routing test fails: 5 vs 144 bytes), the continuation's `toOwnedSlice` failure turned into a panic
  (the sweep's third loop panics; the first two loops did not reach it, which is why the third exists).
- **state**: CLOSED (task S7-8).
- **reversal (P7-W2 T2, upstream `dd2edd760`, DECISIONS B1/B2)**: the non-null empty sentinel above is
  gone. Upstream now returns NULL with length zero for empty `format_alloc`, `selection_format_alloc` and
  `continuation_alloc` output (and `ghostty_alloc(.., 0)` returns NULL), because an empty Zig slice's
  sentinel address (Rust's dangling address is the same hazard) is rejected by foreign runtimes such as
  Go's stack scanner even at length zero. `allocator::into_c_bytes` exports an empty buffer as NULL,
  `allocator::alloc(_, 0)` returns NULL before the allocator is resolved, and borrowed empty strings
  (`rustty_lib::String::init`) point at the one-byte `rustty_lib::EMPTY` static (Zig's `""`).
  `encode_alloc` keeps upstream's unconditional `bytes.ptr`, but its output is never empty (the header
  record), so the shared `into_c_bytes` is equivalent there. Tests: the six upstream tests ("String.init
  empty output", "alloc zero length", "format_alloc empty output", "selection_format_alloc empty output",
  the extended "continuation buffer and allocator export exact suffix", "kitty_image_medium_temp_file empty
  output"); the Rustty-only guards `custom_allocator_zero_length_short_circuit`,
  `format_alloc_empty_output_never_reaches_the_c_allocator` and `format_alloc_null_allocator_round_trip`
  now assert NULL and keep their "no zero-length allocator callback" halves.

## N332 — `snapshot::encode` takes the reference's allocator again; the record scratch spills through it

- **id**: N332. Not a ledger row: S7-8 audit.
- **severity**: low. **kind**: allocator routing, signature fidelity.
- **oracle**: `src/terminal/snapshot/snapshot.zig:44-57` (`encode(alloc, destination, t, options)`:
  `std.heap.stackFallback(512, alloc)` under `record.Writer.init(stack_alloc.get(), destination)`),
  `record.zig:150-170`; the C API passes `native.gpa()` (`c/snapshot.zig:565`, `:596`, `:606`, `:645`), the
  benchmark its own allocator (`benchmark/TerminalSnapshot.zig:183`, `:210`); at 20abdb50a.
- **rustty (before)**: `snapshot::encode(destination, t, options)` dropped `alloc`; `record::Writer`'s
  stack-fallback scratch spilled to the Rust heap.
- **fix**: `crates/rustty-vt/src/snapshot/codec.rs` `encode(alloc, destination, t, options)`; `record::Writer::init_in
  (alloc, destination)` builds the scratch with `StackFallbackWriter::new_in(alloc)` (S7-4's fallible spill);
  `record::Writer::init` keeps the Rust heap for the record-level tests. vt-c passes the terminal's `gpa()`
  (read through raw pointers, no `&mut Terminal` retag), rustty-bench its `alloc`.
- **T3 residual**: the spill is one exact allocation when a payload crosses 512 bytes (513 in the routing test),
  then the handle's growth; the reference's `ArrayList` inside the stack-fallback allocator can spill a little
  earlier and grows by `growCapacity` (as recorded for S7-4's `StackFallbackWriter`).
- **verification**: `encode_alloc_writer_routes_through_the_callers_allocator` pins the terminal side: 2
  allocations (the continuation copy, 129 since S7-8b; the scratch spill, 513), 2 remaps, 2 frees, live bytes unchanged.
  Negative control (`init` in place of `init_in` in `encode`): fails (`left: 2, right: 1`). The C-API writer
  sweep fails the spill on the terminal's allocator (`encode_alloc` reports OUT_OF_MEMORY).
- **state**: CLOSED (task S7-8).

## N333 — the selection-gesture event's codepoint copy goes through the event's allocator (F-008 Tier 2 item 3)

- **id**: N333. Not a ledger row: `DEFERRALS.md` F-008 Tier 2 item 3, design task S7-8.
- **severity**: low. **kind**: allocator routing; reference defect handled (UD314).
- **oracle**: `src/terminal/c/selection_gesture.zig:23-126` (`GestureWrapper`/`EventWrapper.alloc`,
  `EventWrapper.deinit` frees a non-empty copy), `:218-278` (`new`/`event_new`: `alloc.create`; `free`/`event_free`:
  `alloc.destroy`), `:710-739` (`trySetWordBoundaryCodepoints`: `event.alloc.alloc(u21, value.len) catch return
  .out_of_memory`, `std.math.cast(u21, cp) orelse return .invalid_value`; `clearWordBoundaryCodepoints`); at
  20abdb50a.
- **rustty (before)**: the wrappers held a `Box<dyn ZigAllocator>` (a Rust-heap box per wrapper for a C
  allocator) and the copy was a Rust-heap `Vec<u32>`, infallible.
- **fix**: `crates/rustty-vt-c/src/selection_gesture.rs`: both wrappers hold the `VtAllocator` handle and
  allocate/free their storage through it; the copy is an `AVec<u32>` on `event.alloc`, one exact
  `try_reserve_exact(len)` request (`@sizeOf(u21)` is 4: `len * 4` bytes, 4-aligned; a zero `len` makes no
  call), `OUT_OF_MEMORY` on refusal. An out-of-range codepoint returns `INVALID_VALUE` and frees the copy (the
  reference leaks it: UD314, R13).
- **verification**: `selection_gesture::tests::event_codepoints_route_through_the_event_allocator` (exact
  sizes and order: 12-byte copy, freed before the 4-byte replacement; zero-length and null clears make no
  call; `event_free` frees the copy then the wrapper; no resize/remap), `event_codepoints_allocation_failure_is_out_of_memory`,
  `event_invalid_codepoint_frees_the_copy`, `gesture_wrapper_routes_through_the_gesture_allocator`,
  `selection_gesture_allocation_failure_sweep` (every index of the 4-allocation lifecycle; each call from the
  failing index on reports OUT_OF_MEMORY, every byte returned). Negative controls, one at a time: the copy on
  the Rust heap (3 tests fail), the leak reinstated (`mem::forget`; the UD314 test fails on the free count, 2
  vs 1), an inexact `try_reserve` (16 vs 12 bytes), the event wrapper on the Rust heap (5 tests fail), the
  copy's OOM turned into a panic (the sweep panics).
- **state**: CLOSED (task S7-8).

## N334 — F-008/W12-01 Tier 2 closed: the audit, and what stays on the Rust heap

- **id**: N334. Not a ledger row: the closure record for `DEFERRALS.md` F-008/W12-01 Tier 2 (task S7-8).
- **severity**: informational. **kind**: allocator routing audit.
- **audit**: every `lib.alloc.default`, `alloc.create/alloc/dupe/free/destroy`, `Writer.Allocating`,
  `stackFallback` and `gpa()` site in `src/terminal/c/*.zig` (non-test) at 20abdb50a was checked against the
  port: wrappers (Tier 1), terminal internals (S7-3/S7-4), render (S7-5), search (S7-6), OSC (S7-7), selection
  gesture, formatter pin map and the allocating writers (S7-8: N330-N333). The snapshot encoder's own encode
  paths make no other allocation in either.
- **stays on the Rust heap, not routed, because the reference makes no caller allocation there** (port-only
  transients): the page formatter's HTML/VT header buffers (`crates/rustty-vt/src/formatter.rs:1723`, `:1749`;
  the reference uses a fixed 1024-byte stack buffer) and its hyperlink URI copy (`:2170`; the reference
  borrows the page's bytes); `StringMap::shallow_copy` (`string_map.rs:108`; the reference's copy is shallow);
  the `Box<dyn ZigAllocator>` adapters that `rustty_lib::allocator::default` builds for a C allocator in the
  Tier 1 wrappers and helpers (`rustty-vt-c/src/terminal.rs:1620`, `:1958`, `render.rs:1354`, `:1549`,
  `cwrap.rs`); the kitty temporary-directory path (`rustty-vt-c/src/terminal.rs:3173-3190`: one `to_vec` plus
  a clone per screen into `TemporaryFileLimit::Enabled { directory: Vec<u8> }`, where the reference copies the
  bytes into `TerminalWrapper.tmp_dir_path`, a fixed `[max_path_bytes]u8` inside the caller-allocated wrapper,
  and the limits borrow it; making the limits borrow is a rustty-vt kitty-type change); the response strings
  and key snapshots listed in N274. These are cleanup candidates (they cost a Rust-heap allocation the reference
  does not make), not routing gaps (added by S7-8b, review S7-8 F1).
- **stays on the Rust heap although the reference uses the passed allocator, and why**:
  `Screen::selection_string_impl`'s output (`crates/rustty-vt/src/screen.rs:4659`, a `Vec` where the
  reference's `Writer.Allocating` is on `alloc`): not reachable from the C API, and every caller
  (rustty-core `surface.rs`, rustty-apprt-c, tests) passes the Rust heap handle, so the allocator is the same.
  `c/terminal.zig` `Io.init`'s `alloc.create(std.Io.Threaded)` is Windows-only (other targets use `TinyIo`,
  no allocation); the port does not build for Windows. The snapshot decoder's `Source` and `ReaderState`
  (`rustty-vt-c/src/snapshot.rs:318-324`, `decoder_new_source`: two `Box`es freed by `decoder_free`): the
  reference keeps `DecoderWrapper.source` inline (`c/snapshot.zig:59-112, 157-176`), so its bytes are part of
  the caller-allocated wrapper; the port makes the same single caller call for a smaller wrapper plus two
  Rust-heap allocations, because the core decoder holds a `&mut` to the source for the wrapper's lifetime and
  that borrow must not overlap accesses to the wrapper (Stacked Borrows; the `DecoderWrapper` PORT NOTE). Added
  by S7-8b (review S7-8 F1).
- **page memory** stays on the page allocator in both (design §1.2).
- **T3**: element sizes (N325), writer call granularity (N331), the record scratch's spill (N332), and
  allocator_api2 `Vec` growth where the port has not switched to `growCapacity` are call-shape residuals, not
  routing; S7-9 records them.
- **state**: CLOSED. `DEFERRALS.md` F-008/W12-01 Tier 2 is marked CLOSED.

## N335 — `passwd.get`'s Flatpak block leaks the pty slave when the host command cannot start (reference defect, R13; UD320)

- **id**: N335. Found by task L-4 while closing LEDGER S112-F5 in a real Flatpak sandbox.
- **severity**: low. **kind**: reference defect (fd leak on an error path), fixed under R13.
- **where**: `src/os/passwd.zig:69-89` at pin `20abdb50a`: `defer pty.deinit()` (which closes only the master,
  `src/pty.zig:189-192`), then `try cmd.spawn(alloc)` / `try cmd.wait()` before the hand `close(pty.slave)`.
- **reachable**: a Flatpak build whose host command cannot start (no `org.freedesktop.Flatpak` access, no session
  bus). MEASURED in the sandbox: both failure modes (no bus: "Cannot autolaunch D-Bus"; bus without access:
  `ServiceUnknown` -> `FlatpakRPCFail`) reach the early return.
- **port**: the pty guard in `crates/rustty-os/src/passwd.rs` `get` closes the slave while it is still owned
  (`PORT NOTE: reference defect (R13)`, UD320). Test
  `passwd::tests::flatpak_spawn_failure_returns_the_host_command_error_and_closes_the_pty`; negative control (the
  slave close removed) leaks `{4: /dev/pts/0}`.
- **correction**: LEDGER S112-F5's T-G note said the pre-guard code "leaked both fds" on a failing spawn/wait/read
  and that the guard fixed it; the guard fixed the master only (measured: without the guard's `deinit`,
  `{3: /dev/pts/ptmx}` leaks); the slave leak is this finding.
- **UD id**: UD320, with its `docs/ghostty-bug-fixes.md` row.
- **state**: CLOSED (parity tail L-4).

## N336 — `TemplateCallbackReturn` told gboolean template callbacks to return `i32`, which aborts under glib-rs

- **id**: N336. Not a ledger row: found by task L-4's GTK signal survey (UD295 follow-up).
- **severity**: low today (latent: no callback used the alias), high if followed (a process abort on the
  signal). **kind**: port defect (wrong guidance in a type alias and its docs).
- **what**: `crates/rustty-apprt/src/gtk/class.rs` carried the reference's `class.zig:348-352` guard ("bound
  function must return c_int instead of bool") as `pub type TemplateCallbackReturn = i32`, documented "must
  return `i32`, never Rust `bool`". A `#[template_callback]` is called through glib-rs's closure marshaller,
  which asserts the returned `GValue`'s type is the signal's return type (glib 0.22 `closure.rs:243-247`); an
  `i32` is `G_TYPE_INT`, so on a `gboolean` signal it panics inside the C callback and aborts.
- **fix**: the alias is `bool` (`TEMPLATE_CALLBACK_RETURN = true`), with a CORRECTION note quoting the old text.
  Test `gtk::class::tests::template_callback_return_answers_a_gboolean_signal` (no display:
  `GApplication::name-lost`, a `gboolean` signal): the handler runs once and its `TRUE` reaches the emitter.
  NEGATIVE CONTROL (VM): the alias back to `i32` aborts the test process with "Closure returned a value of
  type gint but caller expected gboolean", then "panic in a function that cannot unwind" (SIGABRT).
- **state**: CLOSED (parity tail L-4).

## N337 — the surface paste `drop` handler aborted on a dropped string that is not UTF-8

- **id**: N337. Not a ledger row: found by task L-4's GTK signal survey.
- **severity**: low (reachability through GTK UNVERIFIED). **kind**: port defect (a panic in a C callback where
  the reference reads bytes).
- **what**: `Surface::drop` (`crates/rustty-apprt/src/gtk/class/surface.rs`) read the string branch with
  `value.get::<String>()`, which glib-rs builds with `CStr::to_str().expect("Invalid UTF-8")` (glib 0.22
  `value.rs:898`); the reference (`surface.zig:2768-2773`) takes `value.getString()`'s bytes with
  `std.mem.span`. GTK's text deserializers convert through a charset converter, so whether GTK can deliver
  such a value is UNVERIFIED.
- **fix**: the bytes are read raw with the existing `raw_string_bytes` (`g_value_get_string`), NULL skipped as
  the reference's `orelse`. Test `drop_pastes_a_non_utf8_string_verbatim` (display, `#[ignore]`d; VM under a
  private headless sway): emitting `drop` on `drop_target` with `b"caf\xe9 \xff\xfe"` answers `true` and the
  string branch passes exactly those bytes to `paste` (a `cfg(test)` thread-local records them). NEGATIVE
  CONTROL (VM): `value.get::<String>()` restored aborts the process ("Invalid UTF-8: Utf8Error { valid_up_to:
  3, .. }", "panic in a function that cannot unwind", SIGABRT).
- **state**: CLOSED (parity tail L-4).

## N338 — the surface's two `GtkDropTarget::drop` handlers read the signal's boxed parameter as the dropped value: drag-and-drop never pasted and never moved a split

- **id**: N338. Not a ledger row: found by task L-4 while writing N337's test (the string branch was never
  reached).
- **severity**: high for the feature (file/text drops and split drag-and-drop silently did nothing on Linux).
  **kind**: port defect (marshalling).
- **what**: GTK declares `drop`'s first parameter as `G_TYPE_VALUE`. A C handler (the reference's `dtDrop` /
  `surfaceDrop`, and gtk4-rs's typed `connect_drop` trampoline) receives the dropped `const GValue *` itself;
  a `#[template_callback]` receives the signal's PARAMETER `GValue`, a box around it, and `&glib::Value`'s
  `FromValue` returns that box unchanged. So in `Surface::drop` every `G_VALUE_HOLDS(FileList / File /
  gchararray)` was false and the handler returned `true` having done nothing, and in `surface_drop`
  `get::<u64>()` always failed, so it returned `false` before `move_split` (UD295's fix made it answer the
  signal, but it could not yet accept a drop). No test had reached either branch: the UD295 test and the
  headed KAT emit values that are rejected either way.
- **fix**: `ext::drop_target_value` unboxes the parameter (`BoxedValue`), and both handlers branch on the
  dropped value (`None`, which GTK never emits, answers `false`). Tests: `ext::tests::
  drop_target_value_unboxes_the_signal_parameter` (no display) and N337's
  `drop_pastes_a_non_utf8_string_verbatim`, which asserts the string branch ran. NEGATIVE CONTROL (VM): the
  unboxing removed from `drop`, that test fails with `left: None` (the string branch never ran). The headed
  KAT `gtk_parity_callbacks::parity_callbacks_do_not_abort` and `surface_drop_answers_the_gboolean_drop_signal`
  pass with the fix. A real pointer-driven drop was not performed (UNVERIFIED end to end; the emission
  matches GTK's `G_TYPE_VALUE` parameter).
- **split move measured (task L-4b, review F1)**: `gtk_split_drop::dropping_a_surface_on_another_moves_the_split`
  (headed, rvm under a private headless sway, `LIBGL_ALWAYS_SOFTWARE=1`): two live surfaces A | B in a real
  window's split tree; `drop` on B's split drop target with A's core id at B's bottom edge answers `true` and the
  tree becomes B above A (vertical). 3/3. NEGATIVE CONTROL: the unboxing removed from `surface_drop` fails at
  `surface_drop accepted the drop`. The emission still stands in for a pointer-driven drop.
- **state**: CLOSED (parity tail L-4).

## N339 — survey: every GTK signal connection's return type, port against reference (UD295 follow-up)

- **id**: N339. Task L-4, TASK B. Port at `tail-l4`, reference at `20abdb50a`, return types from the VM's
  `Gtk-4.0`/`Gdk-4.0`/`Gio-2.0`/`GObject-2.0`/`Adw-1` GIR (glib 0.22.8, gtk4 0.11.4, libadwaita 0.9.2).
- **scope**: 104 blueprint bindings (the reference's 1.2-1.5 blueprints, same set; 28 `notify::*`), 40
  production code connections (17 typed `connect_*`, 19 `connect_local`, 1 `connect_closure`, 1 GSignalGroup
  closure, 1 raw `g_signal_connect_data`, the width/height pair counted twice), 3 GDBus subscriptions, 21 custom
  signals (all `void` on both sides), 14 test-only connections.
- **value-returning signals (16 bindings, all return the declared type in the port)**: `GtkGLArea::render` x2
  (bool), `GtkEventControllerKey::key-pressed` x3 (bool), `GtkEventControllerScroll::scroll` x3 (bool),
  `GtkDropTarget::drop` x2 (bool), `GtkDropTarget::motion` (`DragAction`), `GtkDragSource::prepare`
  (`ContentProvider`), `GtkWindow::close-request` (`Propagation`), `AdwTabView::close-page` (`Propagation`),
  `AdwTabView::create-window` (`TabView`), `AdwTabOverview::create-tab` (`TabPage`). Every untyped code
  connection is on a `void` signal and returns `None`.
- **reference**: one `void` handler on a value-returning signal, `surfaceDrop` on `GtkDropTarget::drop`
  (UD295, already fixed); every other value-returning reference handler returns `c_int` or the right pointer.
  No new reference defect, so no new UD.
- **port defects found**: N336, N337, N338 (above), all fixed.
- **not audited**: handler bodies beyond explicit `unwrap`/`expect`/`panic!`/`unreachable!`; class vfunc
  overrides (typed by gtk4-rs). The `core().expect(..)` sites in `surface_drag_prepare`/`surface_drop` and
  `Application::default()` in `on_tab_create_window` are the reference's `.?` invariants (R7).
- **state**: CLOSED (parity tail L-4).

## N340 — on Windows `EnvMap` looks `PATH` up case-sensitively, and Windows names it `Path`

- **id**: N340. Not a ledger row: found by task W-1's first Windows run of `rustty-os`.
- **severity**: medium (Windows). **kind**: port defect (behavioral difference).
- **finding**: `rustty_global::EnvMap` is a plain `HashMap<Vec<u8>, Vec<u8>>`. Zig's environment map compares
  keys case-insensitively on Windows, where environment variable names are case-insensitive: it hashes and
  compares by `toUpperWtf16` over the WTF-8 codepoints (`std/process/Environ.zig:107-141`,
  `std/os/windows.zig:3754` `eqlIgnoreCaseWtf8`). The Windows
  process environment names the search path `Path`, so `path::expand`'s `environ_map.get("PATH")` misses it and
  every lookup returns `null`. MEASURED on rwin (Windows 11 build 26200, ARM64, `x86_64-pc-windows-msvc`):
  `path::tests::expand_hostname` (the reference's own `expandHostname` test) fails with `unwrap()` on `None`; the
  same binary passes when the variable is re-created as `PATH` in a fresh process environment. Every other
  `EnvMap` lookup of a variable Windows spells differently is affected the same way.
- **fix direction** (R1): an `EnvMap` newtype whose key equality and hash use the Windows Unicode upper-case
  fold (`toUpperWtf16`), as Zig's map does, which changes the type behind 219 uses in 38 files; too wide for W-1.
  The reference's `expandHostname` test is the regression test.
  CORRECTION (task W-1b, review W-1 F3): this said the map folds "ASCII case"; Zig's fold is Unicode upper-case.
- **state**: CLOSED (fixed, task W-5). `rustty_global::EnvMap` is a newtype over a `HashMap` keyed by the
  reference's `EnvNameHashContext`: byte equality off Windows, and on Windows `eqlIgnoreCaseWtf8` with a hash of
  the `toUpperWtf16` fold (the Windows arm binds ntdll `RtlUpcaseUnicodeChar`, as Zig does). `insert` is Zig's `put`
  (an existing entry keeps its key spelling), `put_move` is `putMove`. The ~38 files that use the map compiled
  unchanged against the newtype's API. On Windows `Environ` lookups use the same rules (`getAlloc` through
  `createMap`, `containsConstant`/`containsUnemptyConstant` through the ported `getWindows`); `getPosix` is
  POSIX-only, as Zig's. macOS/Linux behaviour is unchanged. Tests (`crates/rustty-global/src/environ.rs`):
  `windows_env_map_names_fold_case_like_the_reference` (Path/PATH, `put`/`putMove` spelling, Λ/λ, ɐ/Ɐ, no fold at
  U+10000+), `windows_eql_ignore_case_wtf8` (Zig's `test "eqlIgnoreCaseWtf16/Wtf8"`), `windows_environ_lookups_fold_case`,
  `windows_create_map_puts_move_the_later_entry`, and on macOS/Linux `env_map_names_are_case_sensitive_off_windows`;
  the reference's `expandHostname` test and `global_environ_map_reflects_the_process` now pass on Windows (rwin,
  `x86_64-pc-windows-msvc`). Negative control on Windows (byte equality and a byte hash in the Windows arm): those
  seven tests fail, `expand_hostname` at `path.rs:342` (`unwrap()` on `None`), as in W-1's measurement.
  Consequence before the fix, by code reading (not measured): `Exec`'s env setup (`exec.rs`, the reference's
  `ghostty_path` block) asks `env.get("PATH")`, missed the `Path` of the process environment, and `put` a second
  variable `PATH` holding only the exe directory, so a Windows child got both `Path` and `PATH`.

## N341 — Windows read-thread quit: a `CancelIoEx` between two reads is lost (reference race)

- **id**: N341. Not a ledger row: found by task W-1 while testing S211-F2 on Windows.
- **severity**: low. **kind**: reference defect (R13, UD326).
- **finding**: `Exec.threadExit` stops the Windows read thread with a quit byte and one
  `CancelIoEx(read_thread_fd)`, ignoring `ERROR_NOT_FOUND` (`Exec.zig:206-227`). `threadMainWindows` checks the
  quit pipe only after a read fails with `OPERATION_ABORTED` (`Exec.zig:1777-1825`). If the cancel lands while
  the thread is between two `ReadFile` calls (processing output), nothing is cancelled, the thread re-enters
  `ReadFile` and blocks until the pseudo console produces more output, and `threadExit`'s `join` waits with it.
  MEASURED: the W-1 test that stops the thread the same way hit `ERROR_NOT_FOUND` in a parallel run (the BEL was
  still being processed); the test now repeats the cancel until the thread returns. Also by reading: any other
  `ReadFile` failure (for example `ERROR_BROKEN_PIPE` once the pseudo console's write end closes) reaches the
  reference's `log.err` + `unreachable` (a panic in the port); not triggered in W-1's runs, UNVERIFIED as reachable.
  The S211-F3 test calls the real `threadExit`, so it carried the same (small) hang risk (review W-1 F4).
- **resolution** (task W-1b, controller ruling R13 with UD325): `threadExit`'s interrupt repeats `CancelIoEx`
  until the read thread has finished (`ReadThread::interruptWindows`), UPSTREAM-DEFECTS UD326, tested by
  `windows_readthread_quit_survives_a_cancel_between_reads` with a negative control (the reference's single
  cancel: the thread is still blocked 5 s later), measured on Windows. This also removes F4's risk. The
  `unreachable`-on-other-errors half stays faithful and is recorded in UD326 (latent, no trigger found).
- **state**: CLOSED (task W-1b; UD326).

## N342 — the application tier does not compile for Windows; CI's Windows leg cannot pass as written

- **id**: N342. Not a ledger row: measured by task W-1 (`cargo check --workspace --all-targets --target
  x86_64-pc-windows-msvc` and `cargo build -p rustty --target x86_64-pc-windows-msvc` on rwin).
- **severity**: medium. **kind**: port defect (R1: a missing platform arm) + CI gap.
  CORRECTION (task W-1b, review W-1 F2): this was kinded "port gap (platform coverage)" as if the reference did
  not build its application tier for Windows. It does: its `test-windows` job runs `zig build -Dapp-runtime=none
  test` (`.github/workflows/test.yml:1711-1724`, the full `ghostty-test` binary: renderer, termio, apprt,
  Command, pty, font, cli, inspector), and `build-libghostty-windows-gnu` (`:959-971`) builds the internal
  library (App + Surface + the OpenGL renderer, `build.zig:212-235`) for the GNU ABI. The reference's OpenGL
  renderer carries `apprt.embedded` no-op arms "to allow libghostty to compile for OpenGL targets"
  (`renderer/OpenGL.zig:172-176, 211-215, 231-233`). So `rustty-core` and everything above it compiling for
  Windows is required by R1. rustty's `ci.yml` Windows header said the opposite; it carries a CORRECTION now.
- **finding**: `rustty-core`'s `renderer_seam` names `rustty_renderer::Renderer`, `generic::Error` and
  `generic::AnimationWakeKind`, which exist only for macOS (Metal), Linux/FreeBSD (OpenGL over EGL) and wasm; the
  reference's Windows renderer is OpenGL (`renderer/backend.zig:20-21`). 10 errors, all in `renderer_seam.rs`.
  So nothing above `rustty-core` builds for Windows: `rustty`, `rustty-cli`, `rustty-apprt`, `rustty-apprt-c`,
  `rustty-bench`, `rustty-extra`, `xtask`. Consequences: CI's Windows step `cargo build -p rustty-apprt-c --release
  --no-default-features` and the `test_dll_init` smoke after it cannot pass; S63-F5's exe link and xtask's
  cmake tests cannot run on Windows. Two workspace members also fail as standalone builds: `libxml2-sys` (its
  vendored config includes `iconv.h`; nothing depends on it on Windows since W-1's apprt fix) and `sentry-sys`
  (the breakpad backend's `client/windows/handler/exception_handler.h` is not vendored). CI's GNU-ABI step was
  not run (no MinGW toolchain on rwin, which was kept lean for disk). The two standalone-member failures are a
  Cargo-workspace artefact (the reference never builds fontconfig/libxml2 on Windows, `font/backend.zig:49-55`,
  nor sentry off macOS, `Config.zig:224-236`); how `cargo build --workspace` should treat them is a controller
  call.
- **work** (not done here; controller: batch W-2): an OpenGL `Renderer` arm for Windows in rustty-renderer (the
  reference's embedded no-op arms), then rustty-core → rustty-apprt-c for `x86_64-pc-windows-msvc` and `-gnu`,
  then a faithful CI leg (a Windows `cargo test` over the crates the reference's `test` covers, the apprt-c
  build for the GNU ABI).
- **resolution** (task W-2): the OpenGL arm now serves Windows too (`any(linux, freebsd, windows)` at every
  gate: `rustty-renderer` crate root, `generic`, `image`, `overlay`, the `gl`/`khronos-egl` dependency table and
  the GLSL staging in `build.rs`), and `generic::linux_impl`'s `surface_init`/`thread_enter`/`thread_exit` switch on
  `rustty_build_config::app_runtime()` as `OpenGL.zig:165-233` switches on `apprt.runtime`, with the reference's
  empty `apprt.embedded` arms (Windows' runtime is `.none`). The compile, link and test fixes this took above the
  renderer are N351. `cargo build`, `cargo clippy --all-targets -- -D warnings` and `cargo test` of the workspace
  now run for `x86_64-pc-windows-msvc` on rwin, except the three members of N350. The `rustty` executable links
  with its resources (LEDGER S63-F5). The GNU-ABI build of `rustty-apprt-c` is in the CI leg but was NOT run
  (no MinGW on rwin: its disk was at the 5 GB floor during the task); UNVERIFIED.
- **state**: CLOSED (task W-2), GNU-ABI half UNVERIFIED.

## N343 — `xev.Async` on Windows is a value, and copies of it do not share a wakeup

- **id**: N343. Not a ledger row: found by task W-1 making `rustty-termio` compile for Windows.
- **severity**: low. **kind**: reference defect candidate (R13 question) + one port defect, fixed.
- **finding**: on kqueue and eventfd backends `xev.Async` is a handle to a kernel object, so a copy notifies the
  same waiter. On IOCP (`watcher/async.zig:533-545`) it is `{guard, wakeup, waiter}` by value. The reference
  copies it: `Termio.init` stores `opts.renderer_wakeup` in the Termio and again in its stream handler, both
  copies of the renderer thread's own Async (`Surface.zig:685`, `Termio.zig:296,316`), so on Windows a notify through either copy sets `wakeup` on the
  copy and never wakes the renderer loop, whose waiter is registered on the original (by reading). The port
  keeps that value semantics (`impl Clone for Async` in `iocp_watcher.rs`, the Zig copy). PORT DEFECT, FIXED:
  `termio::Thread::threadMain_` copied the mailbox's `wakeup` out before `wait` (harmless for a handle); on
  Windows the wait would have been registered on a copy that no notify reaches. It now waits on the mailbox's
  own Async through a pointer, as the reference's `|*v|` capture does.
- **ruling** (controller, task W-1b): record only. At the pin the reference's Windows (embedded OpenGL) renderer
  is a documented no-op ("libghostty is strictly broken for rendering on this platforms", `OpenGL.zig:172-176`),
  so a missed renderer wakeup is not observable; the faithful value semantics stay. Re-open for an R13 ruling if
  a Windows render path is ever ported.
- **state**: CLOSED (the port defect fixed in W-1; the reference question recorded, no change).

## N344 — Windows compile, link and test fixes made by task W-1

- **id**: N344. Not a ledger row: the fixes that made the portable tier build, link and pass on Windows
  (`x86_64-pc-windows-msvc`), each measured on rwin.
- **build**: `simdutf-sys`, `highway-sys` and `rustty-simd` compile with clang-cl on MSVC targets
  (`prefer_clang_cl_over_msvc`): the reference compiles them with Zig's clang, and cl.exe rejects their
  GCC/clang attributes and builtins. `dcimgui-sys` defines `IMGUI_IMPL_API` on Windows as the reference does
  (`extern "C" __declspec(dllexport)`, `pkg/dcimgui/build.zig:73-80`); without it the OpenGL3 backend had C++
  linkage and failed the link. `rustty-apprt`'s `fontconfig-sys` edge is off Windows, like
  `rustty-font-shaper`'s (`font/backend.zig:49-55`: no fontconfig on Windows); it was dragging `libxml2` into
  every crate above apprt.
- **link**: `SystemFunction036` (advapi32) and `AcceptEx` (mswsock) get their libraries, as Zig's std declares
  them; `rustty-os::i18n`'s `build_config.i18n` checks are inline `const` (R8: the reference's `comptime`), so a
  build with i18n off (Windows) does not reference libintl.
- **code**: `rustty-font-shaper` matches FreeType's error and render-mode constants in bindgen's own type
  (`c_int` on MSVC, `c_uint` elsewhere). `rustty-renderer`'s shader-file open converts WTF-8 path bytes on
  Windows. `rustty-termio`'s Windows arms: the child pid type, the quit pipe's `posix.system.close`/
  `posix.system.write` as `CloseHandle`/`WriteFile` (the reference's calls are the C runtime's, which crash on a
  HANDLE: R13, UPSTREAM-DEFECTS UD325; CORRECTION (task W-1b, review W-1 F1): this said "`posix.close`/
  `posix.write`", which would be Zig's own Windows mapping), `termiosTimer` and `Subprocess.childPreExec` compiled out on Windows (Zig never
  analyzes them there), an inert placeholder `ThreadData`, `OsPasswd.get` unreachable on Windows (the
  reference's `@compileError`, R7). `rustty-xev` on Windows: `Completion::state_of`, the unprefixed
  `WaitError` export, `Clone` for `Async` (N343). `rustty-os::TempDir::realpath` returns Zig's DOS form, not
  a verbatim `\\?\` path (13 shell-integration tests failed on the verbatim form).
- **tests**: compile-time scopes (R14 form 1) for tests of POSIX-only items (`expandHomeUnix`, `passwd.get`,
  the systemd `/proc` helpers, the empty-`TMPDIR` child test, the pagelist RSS bench, a non-UTF-8 `OsStr`
  fixture); test path bytes via `as_encoded_bytes`; the Windows expectation of the NUL-argument `execCommand`
  test (the Windows shell arm tokenizes instead of wrapping in `/bin/sh -c`).
- **lint**: `cargo clippy --all-targets -- -D warnings` for `x86_64-pc-windows-msvc` over rustty-os, -termio,
  -xev, -vaxis, -font-shaper, -simd, -vt, -vt-c (and rustty-config below them) was red on Windows-only lints
  (Win32 type spellings, dead code of POSIX-only helpers, comptime-dead tails); fixed with scoped
  `cfg`/`cfg_attr` allows or by compiling POSIX-only items out, each with its reason at the site. Green on rwin.
- **W-1b additions**: the reference's MSVC `-fno-autolink`/`-fno-stack-protector` for simdutf and
  `-D_Avx2WmemEnabledWeakValue=_Avx2WmemEnabled` + `-fno-autolink` for the SIMD sources (`SharedDeps.zig:995-998`,
  missing in `rustty-simd` before) reach clang-cl as `/clang:` flags (MEASURED on the compile lines with
  `CC_ENABLE_DEBUG_OUTPUT=1`; the bare spellings were dropped by `flag_if_supported`); rustty-simd's tests pass on
  Windows (42/42). `TempDir::realpath` strips `\\?\` only before a drive letter (a `\\?\Volume{GUID}\` path
  has no DOS form; test `dos_form_strips_only_drive_and_unc_prefixes`, negative control W-1's unconditional strip
  fails). The lint allow on `impl ReadThread` is narrowed to its six POSIX-only constants.
- **state**: CLOSED (informational). Results in the W-1 and W-1b reports.

## N345 — rustty-vt's "real Io" has no entropy on Windows, so five vt tests fail there

- **id**: N345. Not a ledger row: measured by task W-1 (`cargo test -p rustty-vt --target x86_64-pc-windows-msvc`
  on rwin, the CI Windows leg's command): 3186 pass, 19 ignored, 5 fail.
- **severity**: medium (Windows CI). **kind**: port gap (test Io).
- **finding**: the reference's tests pass `testing.io` (`std.Io.Threaded`), whose `randomSecure` has a Windows
  source. The port maps it to `sys::io_native`, which is `rustty_os::file::random_secure`, the port of
  `TinyIo.randomSecure`: it has no Windows arm, because TinyIo is `!supported` on Windows (`TinyIo.zig:35-38`).
  So on Windows `kitty::clipboard_grants::tests::generate_otp_length_and_alphabet_with_a_real_io` and
  `stream_terminal::tests::{paste_reader_contents_are_read_on_demand, paste_mode_5522_sends_an_event_the_program_can_read_with,
  paste_mode_5522_reports_the_selection_as_primary}` fail `EntropyUnavailable`, and
  `paste_mode_5522_without_entropy_fails_and_records_no_grant` then fails on the `random_secure_test_lock`
  those panics poisoned (`PoisonError`). The library behaviour itself (no entropy through TinyIo on Windows) is
  the reference's; the application-tier Io that the tests stand for is what is missing.
- **vt-c cascade (UNVERIFIED cause, not traced)**: most likely the same entropy gap takes down `rustty-vt-c`'s `paste::tests::terminal_paste_event` on Windows (`left: Success
  right: IoError`), and six more `paste::tests` then fail on the `PASTE_TEST_LOCK` it poisoned (`PoisonError`);
  measured by `cargo test -p rustty-vt-c --target x86_64-pc-windows-msvc` (531 pass, 8 fail; the eighth is N346).
- **resolution** (task W-2): the reference's Io on Windows is `std.Io.Threaded` in EVERY artifact: the application's
  `global.io()`, the tests' `testing.io`, and libghostty-vt's C API (`terminal/c/terminal.zig:57-67`: "On Windows
  we use `std.Io.Threaded` since TinyIo doesn't implement the NT operations"). So the port's native Io
  (`rustty_vt::sys::io_native`, the default of every `Terminal`, in the app and in vt-c alike) reads Threaded's
  Windows source on Windows: `rustty_os::file::random_secure_threaded`, the port of `Threaded.randomSecure`'s
  Windows arm (`NtDeviceIoControlFile(\Device\CNG, IOCTL_KSEC_GEN_RANDOM)`, `std/Io/Threaded.zig:17011-17054`,
  `getCngDevice` `:15792-15835`; the handle is cached process-wide). The five vt failures and the vt-c paste
  cascade (7 tests, confirmed the same cause: they pass with the fix) are gone; test
  `random_secure_threaded_fills_with_fresh_entropy` (rustty-os, Windows). Negative control on Windows: the first
  W-2 full run (the same tree without this fix) failed exactly those 12 tests, with `EntropyUnavailable` and the
  poisoned locks, and the core paste test S42-F1 (`completeClipboardPasteEvent` returned the entropy error).
- **state**: CLOSED (task W-2). (Was: OPEN (controller: the vt test Io on Windows, and whether the application artifact's Io needs a Windows
  entropy source).)

## N346 — two Windows test-harness gaps outside W-1's rows (rustty-vt-c, rustty-vaxis)

- **id**: N346. Not a ledger row: measured by task W-1 on rwin (`x86_64-pc-windows-msvc`).
- **severity**: low. **kind**: port gap (test scaffolding on Windows).
- **rustty-vt-c**: `terminal::tests::compression_activity_and_incremental_scheduling` hits its
  `.unsupported => unreachable` arm: `mem::can_reclaim` answers `true` under `cfg!(test)` only for rustty-vt's
  own unit tests, while Zig's `builtin.is_test` holds for the whole test binary (`terminal/mem.zig:37-42`), and
  Windows has no OS reclamation. The crate-external switch `test-decommit-sim` exists (rustty-bench enables it)
  but rustty-vt-c's tests do not; on macOS and Linux the test passes through the OS path instead.
- **rustty-vaxis**: `loop_::tests::{loop_lifecycle, queued_key_text_is_owned_not_a_view_of_the_read_buffer}` fail
  `InvalidHandle`: on Windows the test build's `Tty` is the real `WindowsTty` (no console under ssh), because the
  port narrowed `TestTty` to non-Windows on the premise that it needs `posix.pipe`; libvaxis 0.6.0's `TestTty`
  (the pin's) is a discarding writer that compiles on Windows (`tty.zig:17-22` selects it on every OS under
  `is_test`). 131 of 133 vaxis tests pass on Windows.
- **resolution** (task W-2): rustty-vt-c's dev-dependency on rustty-vt enables `test-decommit-sim` (Zig's
  whole-binary `builtin.is_test`), and rustty-vaxis's test alias is `TestTty` on every target, with a Windows
  `TestTty` that ports libvaxis 0.6.0's discarding-writer arm (`tty.zig:814-842`). Both suites pass on Windows.
  Consequence (review W-2 F7, info): `cargo test -p rustty-vt-c` on macOS/Linux now takes the simulated
  reclamation too, as the reference's test binary does; the real Darwin/Linux `madvise` paths are exercised only by
  the bench binary (the reference does not test them either).
- **state**: CLOSED (task W-2).

## N347 — under clang-cl, `flag_if_supported` still drops the reference's other `-f` flags (highway, simdutf)

- **id**: N347. Not a ledger row: measured by task W-1b on rwin (`CC_ENABLE_DEBUG_OUTPUT=1` compile lines,
  `x86_64-pc-windows-msvc`), while checking review W-1 F6.
- **severity**: low. **kind**: port gap (build flags on MSVC targets).
- **finding**: clang-cl does not accept every clang driver flag in its bare spelling, and `cc`'s
  `flag_if_supported` silently drops the ones it rejects. After W-1b's `/clang:` fix of the MSVC-only flags, still
  missing from the Windows compile lines: `highway-sys`'s `-fno-cxx-exceptions`, `-fno-slp-vectorize`,
  `-fno-vectorize` (`pkg/highway/build.zig` flag list; `-fmerge-all-constants` and `-Wno-builtin-macro-redefined`
  do get through) and `simdutf-sys`'s `-fno-exceptions`, `-fno-rtti` (the `no_libcxx` pair). macOS and Linux
  are unaffected (cc drives clang there).
- **fix direction**: pass them as `/clang:<flag>` on MSVC targets (as W-1b does for the MSVC-only flags), then
  re-run the SIMD and vt suites on Windows. Not done in W-1b: each changes code generation (vectorization,
  exception tables) of code the Windows suites exercise, and needs its own measured run.
- **resolution** (task W-2): the five flags go through `/clang:` on MSVC targets (`highway-sys`'s
  `-fno-cxx-exceptions`/`-fno-slp-vectorize`/`-fno-vectorize`, `simdutf-sys`'s `-fno-exceptions`/`-fno-rtti`), and
  highway's own MSVC arm (`pkg/highway/build.zig:102-108`: `-D_Avx2WmemEnabledWeakValue=_Avx2WmemEnabled`,
  `-fno-autolink`, `-fno-stack-protector`), which was not mirrored at all, is added. MEASURED on rwin (`CC_ENABLE_DEBUG_OUTPUT=1`, release build of `rustty-bench` for `x86_64-pc-windows-msvc`):
  highway's compile line gains `/clang:-fno-slp-vectorize`, `/clang:-fno-vectorize`, `/EHs-c-`,
  `-D_Avx2WmemEnabledWeakValue=_Avx2WmemEnabled`, `/clang:-fno-autolink` and `/clang:-fno-stack-protector`; simdutf's
  gains `/clang:-fno-exceptions` and `/clang:-fno-rtti`. `-fno-cxx-exceptions` has no clang-cl spelling (the
  driver ignores it even through `/clang:`: "argument unused during compilation"); clang-cl's switch for it is
  `/EH`, and with no `/EH` at all (cc adds none) C++ exceptions were already off (measured: `throw` is rejected),
  so `/EHs-c-` states it without changing code. Timing (9 runs each, median; 16 MiB of mixed UTF-8 and SGR data;
  x64 binaries under Windows' ARM64 emulation, so the absolute numbers say little): `+codepoint-width --mode=simd`
  131.7 ms before / 130.5 ms after; `--mode=table` (a control without SIMD) 58.8 / 59.3; `+terminal-stream`
  391.9 / 387.7. No difference above the noise. The SIMD, highway and simdutf suites pass on Windows with the
  flags (42 + 18 + 2).
  NOT CHECKED (review W-2 F6): highway's `-fmerge-all-constants`, `-fno-sanitize=undefined` and
  `-fno-sanitize-trap=undefined` are still passed BARE through `flag_if_supported`. The measured compile line
  above does list all three (so clang-cl accepted them there), but whether clang-cl honours or merely tolerates
  each was not established; confirm with `CC_ENABLE_DEBUG_OUTPUT=1` (and `-###`) on Windows and route any
  ignored one through `/clang:` (left for W-4/W-3; task W-2b had no Windows host).
- **task F-1** (the W-2 F6 residual): all three are honoured by clang-cl in their bare spelling, MEASURED on rwin
  with `clang-cl -###` (LLVM 23.1.2): `-fmerge-all-constants` appears on the `-cc1` line (absent without it);
  `-fsanitize=undefined -fsanitize-trap=undefined` gives `-fsanitize=...` and `-fsanitize-trap=...`, adding
  `-fno-sanitize-trap=undefined` removes the `-fsanitize-trap=` set (recover instead), adding
  `-fno-sanitize=undefined` removes every `-fsanitize*` argument. No warning for any of them. Nothing to reroute; the
  same measurement covers the bare `-fno-sanitize*` pair in every other `-sys` build (N353).
- **state**: CLOSED (task W-2; residual confirmed by task F-1).

## N350 — three workspace members are excluded from the Windows build, explicitly

- **id**: N350. Not a ledger row: task W-2 (N342's standalone-member question, a controller call it was handed).
- **kind**: build scope (documented exclusion, not a gap).
- **finding**: `libxml2-sys` and `fontconfig-sys` (its only dependent) and `sentry-sys` do not build for
  `x86_64-pc-windows-msvc` as standalone workspace members (`libxml2`'s vendored config includes `iconv.h`;
  breakpad's `client/windows/handler/exception_handler.h` is not vendored). The reference never builds them for
  Windows: its Windows font backend is `freetype_windows` (`font/backend.zig:49-55`, no fontconfig/libxml2) and it
  builds sentry only for Darwin (`build/Config.zig:224-236`). Nothing in the Windows dependency graph reaches them
  (`cargo tree --target x86_64-pc-windows-msvc -i libxml2-sys` / `-i sentry-sys`: roots only).
- **decision**: every Windows workspace command excludes exactly these three, by name:
  `--exclude libxml2-sys --exclude fontconfig-sys --exclude sentry-sys` (inline on the CI leg's build, clippy and test
  steps, with this reason at the job; they were a `WINDOWS_EXCLUDE` variable until task W-2b). `breakpad-sys` builds on Windows as a member (with cl.exe dropping `-std:gnu++17`, N353) and
  is not excluded, although the reference does not build it there either.
- **state**: CLOSED (documented; `.github/workflows/ci.yml` `windows` job).

## N351 — Windows compile, link and test fixes made by task W-2

- **id**: N351. Not a ledger row: the changes that made the workspace above the portable tier build, lint and test
  on Windows (`x86_64-pc-windows-msvc`), each measured on rwin (Windows 11 build 26200, ARM64, x64 emulation).
- **renderer** (N342): the OpenGL arm serves Windows, with the `apprt.embedded` no-op arms; `prepBackgroundImage`
  opens the WTF-8 path as Zig's `openFile` does (was `OsStrExt`, unix-only).
- **core**: `cursorCancelCallback` has an IOCP arm (libxev's IOCP `CancelError` is `error{Unexpected}`, the
  reference's `else`; same handling as the Linux arm, UD73; the second static copy of the warning is declared in
  `LOG-PORT-ONLY.tsv`); the by-value `xev.Async` copies (`renderer_wakeup`, `draw_now`) are `.clone()` (Zig's value
  copy; on IOCP an Async is a value, N343); the S42-F1 paste test waits on the mailbox's own Async through a pointer
  (a copy's waiter is never notified on IOCP).
- **apprt**: `Surface.init`'s `openDirAbsolute` has a Windows arm (Zig's `dirOpenDirWindows`: WTF-8 path, a
  directory-only open that fails `NotDir` on a file); the `rlimit` stand-ins of `rustty-os` and `rustty-global`
  are one type on Windows (`()`), so `ResourceLimits.init(fixMaxFiles())` compiles.
- **cli**: `std.fs.max_path_bytes` buffers use `rustty_os::path::MAX_PATH_BYTES` (was `libc::PATH_MAX`, POSIX
  only); `+edit-config`'s code after the reference's comptime Windows `return 1` is compiled out on Windows; the
  `--working-directory` realpath converts WTF-8 and returns the DOS form, and `realPathFile`'s Windows error names
  are mapped (`dirRealPathFileWindows` + `OpenFile`'s NTSTATUS table; they were all `Unexpected`).
- **realpath DOS form** (`rustty_os::path::realpath`): Zig's `realpath` on Windows returns the DOS form
  (`GetFinalPathNameByHandle(.Dos)`); `std::fs::canonicalize` returns `\\?\C:\...`. MEASURED consequence: the kitty
  temporary-file medium rejected every in-directory file on Windows (`TemporaryFileNotInTempDir`: the file's
  canonical path kept the verbatim prefix, the handle's did not). The config loader's path expansion, its cwd
  realpath, `TempDir.realPath` and `+validate-config` use it too.
- **config**: the two `assert(std.fs.path.isAbsolute(path))` sites use the ported Zig `isAbsolute`
  (`crate::path::is_absolute`); Rust's `Path::is_absolute` is false on Windows for a rooted `/x` that Zig calls
  absolute (it aborted the whole rustty-config test binary on Windows).
- **xev**: `backend()` has the Windows arm (`.iocp`, single candidate, as kqueue on macOS).
- **checkout**: `.gitattributes` keeps `crates/rustty-extra/testdata/**` (`-text`) and `dist/linux/**` (`eol=lf`)
  LF on a Windows checkout; under `* text=auto` they were CRLF there and the extra goldens and Linux-resource tests
  failed.
- **tests** (R14 form 1, each with its reason at the site): POSIX-only on Windows — the list-themes pty test,
  `load_path_errors_propagate.rs` (`HOME`; Windows reads `LOCALAPPDATA`), the apprt-c tests that need
  `rustty_init` to succeed (the reference's `ghostty_init` returns 1 on Windows, `UnsupportedOSForCApi`; new test
  `rustty_init_is_unsupported_on_windows`) and its C smoke (Windows smoke = `test_dll_init.c`), the i18n catalog
  test (i18n is off on Windows, `Config.zig:263-271`), zf's `/`-path tests (UD330), xtask's four `tic` tests (no
  ncurses on the host; the reference's Windows build compiles no terminfo); Windows arms — `panic_policy`'s abort
  status (`0xC0000409`), the bench `ensureLocale` line (the CRT ignores `LANG`), the NTFS-dated crash report, the
  `new_window` error name, the CMake install layout and `.pc` checks (xtask), the OpenGL gate guard (now requires
  `windows` and reads wrapped attributes whole; negative control: the `windows` target removed from one wrapped
  gate fails the guard).
- **results** (x86_64-pc-windows-msvc, rwin): final `cargo test --workspace` (the three N350 members excluded, 194 test binaries): 5982 passed, 4 failed, 69 ignored. Per crate (passed/failed/ignored): dcimgui-sys 5/0/0; freetype-sys 1/0/0; glslang-sys 1/0/0; harfbuzz-sys 1/0/0; highway-sys 18/0/0; libpng-sys 2/0/0; oniguruma-sys 1/0/0; rustty 16/0/0; rustty-apprt 30/0/0; rustty-apprt-api 27/0/0; rustty-apprt-c 26/0/0; rustty-args 71/0/10; rustty-bench 47/0/1; rustty-build-config 3/0/0; rustty-cfg-fmt 9/0/0; rustty-cli 102/0/5; rustty-config 259/0/0; rustty-core 72/0/0; rustty-crash 20/0/0; rustty-datastruct 87/0/2; rustty-extra 36/0/0; rustty-font 89/0/0; rustty-font-shaper 105/0/3; rustty-global 8/2/1; rustty-helpgen 6/0/0; rustty-identity 1/0/0; rustty-input 117/0/1; rustty-inspector 23/0/0; rustty-lib 41/0/2; rustty-os 86/1/8; rustty-render-types 12/0/0; rustty-renderer 124/0/0; rustty-simd 42/0/0; rustty-synthetic 27/0/0; rustty-terminfo 7/0/0; rustty-termio 67/1/14; rustty-unicode 20/0/0; rustty-util 6/0/0; rustty-vaxis 157/0/0; rustty-version 5/0/0; rustty-vt 3268/0/17; rustty-vt-c 554/0/1; rustty-wasm 0/0/1; rustty-web-canvas 1/0/0; rustty-wuffs 19/0/0; rustty-xev 43/0/0; rustty-z2d 105/0/0; rustty-zf 7/0/0; rustty-zigimg 28/0/0; rustty-zigstd 85/0/0; simdutf-sys 2/0/0; spirv-cross-sys 1/0/0; wuffs-sys 3/0/0; xtask 78/0/3; zlib-sys 11/0/0. The 4 failures: `rustty-os` `path::tests::expand_hostname` and `rustty-global` `environ::tests::global_environ_map_reflects_the_process` (N340, `Path` vs `PATH`), `sync_environ_makes_a_post_capture_mutation_visible` (N352), and `rustty-termio` `windows_threadenter_reads_the_pty_through_the_windows_read_thread` (N354; it passed in the two earlier full runs and 3/3 alone). Before W-2 (first full run, same host): 5682 passed, 55 failed, and the whole `rustty-config` unit binary aborted (its `isAbsolute` assert).
- **state**: CLOSED (informational).

## N352 — on Windows the port's global `Environ` is a snapshot, Zig's reads the process environment live

- **id**: N352. Not a ledger row: found by task W-2 (`sync_environ_makes_a_post_capture_mutation_visible` fails on
  Windows).
- **severity**: low. **kind**: port defect (R1, Windows only).
- **finding**: Zig 0.16's `std.process.Environ` on Windows is a `GlobalBlock` (`use_global`), and `createMap` reads
  the live PEB environment block every time (`std/process/Environ.zig:20-34, 482-491`); the reference's
  `global.init` gives the global state that block (`.{ .block = .{ .use_global = true } }`), so `environMap()` sees
  every later `SetEnvironmentVariable`, which is why `syncEnviron` is a no-op there (`global.zig:307-321`). The port
  captures a snapshot on every target (`rustty_global::environ`), so on Windows a mutation after the capture is
  invisible and `sync_environ` (faithfully a no-op there) cannot refresh it.
- **fix direction**: a Windows `Environ` that reads the live environment on each `create_map`/lookup (the Zig
  `GlobalBlock`), keeping explicit `from_entries` environments for tests; belongs with N340's `EnvMap` change (same
  owner, same types).
- **state**: CLOSED (fixed, task W-5). On Windows `Environ` carries `use_global` (Zig's `GlobalBlock`):
  `Environ::from_process()` is `.{ .use_global = true }` and captures nothing, and `create_map`/`get_alloc`/
  `get_windows`/`contains_*` read the live process environment (`std::env::vars_os`, which is
  `GetEnvironmentStringsW`) on every call; `Environ::default()` is `GlobalBlock.empty`; `from_entries` stays for tests.
  `sync_environ` stays a no-op there. Tests: `sync_environ_republishes_from_process.rs` has a Windows arm (a variable
  set after the first lookup is visible at once, case-insensitively, and a removal too; `syncEnviron` changes
  nothing) and `windows_process_environ_is_the_global_block`. Negative control on Windows (`from_process` back to a
  snapshot): both fail, the first with `left: Some("before") right: Some("after")` ("environ_map() must read the live
  environment on Windows (N352)"). Not changed: `rustty-font-shaper`'s Windows discovery reads `SYSTEMROOT` and
  `LOCALAPPDATA` with `std::env::var_os` where the reference uses `global.environ().getAlloc` (no `rustty-global` edge
  there); both are live, case-insensitive reads, so the behaviour is the same.

## N353 — on MSVC targets most vendored C/C++ dependencies are compiled by cl.exe, the reference compiles them with clang

- **id**: N353. Not a ledger row: measured by task W-2 on rwin (`cargo:warning` lines of the Windows build).
- **severity**: low. **kind**: port gap (build fidelity on MSVC targets).
- **finding**: only `simdutf-sys`, `highway-sys` and `rustty-simd` set `prefer_clang_cl_over_msvc` (N344). Every other
  `cc` build (`zlib-sys`, `freetype-sys`, `harfbuzz-sys`, `libpng-sys`, `oniguruma-sys`, `glslang-sys`,
  `spirv-cross-sys`, `wuffs-sys`, `dcimgui-sys`, `gettext-sys`, `breakpad-sys`, and the `rustty-os`/`rustty-apprt`/
  `rustty-font-shaper` shims) uses cl.exe on an MSVC target, while the reference compiles every C/C++ unit with Zig's
  clang, `abi == .msvc` included. Observable: cl.exe ignores clang flags passed with `.flag()` (MEASURED: `zlib-sys`
  `-fno-sanitize=undefined`, `-fno-sanitize-trap=undefined`; `breakpad-sys` `-std:gnu++17`, D9002), and
  `flag_if_supported` drops others silently. Everything builds and its tests pass; the difference is the compiler.
- **fix direction**: `prefer_clang_cl_over_msvc(true)` in each build, the clang-only flags through `/clang:` (the
  N347 pattern), then a measured Windows run per dependency.
- **fix** (task F-1): `prefer_clang_cl_over_msvc(true)` in `zlib-sys`, `freetype-sys`, `harfbuzz-sys`, `libpng-sys`,
  `oniguruma-sys`, `glslang-sys`, `spirv-cross-sys`, `wuffs-sys` (codec and shim), `dcimgui-sys`, `gettext-sys`,
  `breakpad-sys`, and the `rustty-os` (pty, os_log), `rustty-apprt` (GResource blob) and `rustty-font-shaper`
  (stb_image_resize) shims. `gettext-sys`, `rustty-os` and `rustty-apprt`'s builds are not reached on an MSVC target
  (macOS-only, POSIX-only, GTK-only), so there it only states the compiler. Flags, each MEASURED on rwin
  (`clang-cl -###`, LLVM 23.1.2):
  - bare `-fno-sanitize=undefined`, `-fno-sanitize-trap=undefined`, `-w` and `/FIcstdint` are honoured by clang-cl
    (N347's task F-1 note), so they stay as they are;
  - `-std=gnu17` (`oniguruma-sys`) is "unknown argument ignored in clang-cl": now `/clang:-std=gnu17` on MSVC;
  - cc spells `breakpad-sys`'s `.std("gnu++17")` as `-std:gnu++17`, which clang-cl ignores ("argument unused during
    compilation", the C++14 default then applies) as cl.exe did (D9002): now `/clang:-std=gnu++17` on MSVC,
    `-std=gnu++17` elsewhere (what `.std()` emitted there);
  - clang-cl enables C++ exceptions only through `/EH`, and cc passes none: `spirv-cross-sys` failed to build
    ("cannot use 'throw' with exceptions disabled", 17 sites). The reference passes no exception flag for any of
    these C++ packages, so Zig's clang uses its MSVC-target default, `-fcxx-exceptions -fexceptions` (MEASURED:
    `clang --target=x86_64-pc-windows-msvc -### -x c++`). `/EHs` gives exactly that pair (`/EHsc` adds
    `-fexternc-nounwind`; nothing gives neither), so `harfbuzz-sys`, `glslang-sys`, `spirv-cross-sys`,
    `dcimgui-sys` and `breakpad-sys` pass `/EHs` on MSVC. No dependency had to stay on cl.exe.
- **evidence** (rwin, `x86_64-pc-windows-msvc`, `CC_ENABLE_DEBUG_OUTPUT=1`):
  `cargo clippy --workspace --exclude libxml2-sys --exclude fontconfig-sys --exclude sentry-sys --all-targets
  -- -D warnings` passes. In every `-sys`/shim build-script output of that build, 0 `"cl.exe"` lines, every compile
  line is `clang-cl.exe`, and 0 `D9002`/"unknown argument"/"argument unused" warnings; the pre-fix `zlib-sys` output
  of the same clone (built by cl.exe) carries the two D9002 lines for `-fno-sanitize=undefined` and
  `-fno-sanitize-trap=undefined`. Sample lines: oniguruma `... "-w" "/clang:-std=gnu17" "-fno-sanitize=undefined"
  "-fno-sanitize-trap=undefined" ...`; breakpad `... "/clang:-std=gnu++17" "/EHs" "-w" ...`; spirv-cross, glslang,
  harfbuzz, dcimgui `... "-std:c++17" "-W0" "/EHs" "-w" "-fno-sanitize=undefined" "-fno-sanitize-trap=undefined"
  ...`. Tests, one run: `cargo test --no-fail-fast -p zlib-sys -p libpng-sys -p freetype-sys -p harfbuzz-sys
  -p glslang-sys -p spirv-cross-sys -p dcimgui-sys -p oniguruma-sys -p wuffs-sys -p rustty-font-shaper
  -p rustty-oniguruma -p rustty-wuffs -p rustty-zigimg -p rustty-renderer -p rustty-inspector`: all pass
  (rustty-font-shaper 109 + 3 ignored, rustty-renderer 121 + `opengl_gate_parity` 3, rustty-wuffs 19,
  rustty-zigimg 16 + codec 12, zlib-sys 11, dcimgui-sys 5, wuffs-sys 3, the other `-sys` smoke tests 1-2 each), except
  two rustty-inspector tests that failed on a missing `test-decommit-sim`, not on the compiler (fixed under N383,
  then 21/21). macOS is unaffected (cc drives clang there; `cargo build --workspace` and the touched `-sys` tests
  pass).
- **not run** (`.port/phase-2/VERIFICATION_DEBT.md` V26): the full Windows `cargo test --workspace` over the clang-cl-built
  dependencies (the rustty-vt suite, the only other zlib/dcimgui consumer, takes several minutes under emulation).
- **state**: CLOSED (fixed; full-workspace Windows test run deferred to V26).

## N354 — `windows_threadenter_reads_the_pty_through_the_windows_read_thread` failed once in a full Windows run

- **id**: N354. Not a ledger row: observed by task W-2 on rwin.
- **severity**: low. **kind**: flaky test (cause not traced).
- **finding**: the W-1 test (LEDGER S211-F3) failed once, in the third full `cargo test --workspace` run, at
  `threadEnter(..).expect("threadEnter")` with `threadEnter: Error` (the error's Debug form carries no detail). It
  passed in the two earlier full runs and 3/3 when `rustty-termio`'s suite ran alone right after. Cause UNVERIFIED:
  the call spawns `cmd.exe` on a fresh ConPTY while the rest of the suite runs in parallel under x64 emulation.
- **next step**: make the test report which step of `threadEnter` failed, then rerun it under load.
- **task W-5**: the reference does not fold: `Exec.threadEnter` returns a distinct error per step
  (`error.ProcessNoPid`, `error.ProcessNotStarted`, `Command.start`'s error, the spawn's `SpawnError`). The port's
  `crate::Error` keeps only the members a caller dispatches on (S212-F1), so the step is reported by a test-only
  diagnostic instead (`note_thread_enter_failure!`, `cfg(test)`, expands to nothing in production): each error
  arm of `threadEnter` and the Windows arm of `Subprocess.start` (`Pty.open`, `Command.start`) record the step and
  the error on the calling thread, and the test's panic prints them. Its positive check:
  `threadenter_read_thread_spawn_failure_drains_before_stop` (macOS) asserts the one recorded step is
  `threadEnter: std.Thread.spawn: ...`. Reproduction attempts on rwin, all passing: 20 `cargo test -p rustty-termio`
  runs (68/68 each); 20 more while rustty-vt/core/os unit tests ran in a loop beside them; the test alone in 4
  parallel processes x 25 and 8 x 20 (260 runs) under that load; 2 full `cargo test --workspace` runs (baseline and
  final). 0 failures in about 300 runs. Cause still UNVERIFIED; W-2's one failure was on the pre-W-5 environment
  code (a snapshot `Environ` and a case-sensitive `EnvMap`), which the ConPTY spawn path reads through `Exec`'s
  env setup; no link to the failure was shown.
- **state**: OPEN (not reproduced; the next failure names its step).

## N355 — a one-off SIGSEGV of `rustty-renderer`'s unit-test binary on macOS

- **id**: N355. Not a ledger row: observed by task W-2 on the Mac.
- **severity**: unknown. **kind**: crash, unreproduced (cause not traced).
- **finding**: in round 2 of W-2's three rounds of `cargo test` over the touched crates, `rustty-renderer --lib`
  died with `signal: 11, SIGSEGV` after `message::change_config_tests::init_change_config_builds_both_derived`
  printed `ok`. Not reproduced since: 36 runs in W-2 (6 `cargo test`, 30 direct), and 670 in review W-2 (50
  `cargo test`, 300 serial direct runs, 40 rounds of 8 concurrent copies, one multi-crate round), all green;
  no crash report in `~/Library/Logs/DiagnosticReports`. W-2 changed no code that compiles into macOS's
  renderer (its renderer changes are `windows` cfg widenings); it may be pre-existing UB, Metal/driver state,
  or environmental.
- **if it recurs**: a signal gives no Rust backtrace (`RUST_BACKTRACE` does not help). Run the binary under a
  debugger loop until it stops, e.g. `for i in $(seq 1 500); do lldb --batch -o run -k 'thread backtrace all'
  -k quit <target/debug/deps/rustty_renderer-HASH> || break; done`, and compare `--test-threads=1` runs to
  separate threading from Metal.
- **task F-1** (15-minute bound, no hunt): the loop above, time-boxed to 870 s, on
  `target/debug/deps/rustty_renderer-<hash>` built from `merge-w4` @ 110ecea9 plus F-1's build-script changes (none
  compiles into the macOS renderer), alternating default threading and `--test-threads=1`, with a
  `cargo build --workspace` and `cargo clippy --workspace` running beside it for part of the window: 496 runs
  (248 + 248), 496 exited 0, no stop. Total without a recurrence: 706 + 496 = 1202 runs.
- **state**: CLOSED (unreproduced, 1202 runs; the recipe is `.port/phase-2/VERIFICATION_DEBT.md` V26).

## N360 — allocator call-shape residuals (T3): every C-observable difference the S-7 series leaves

- **id**: N360. Not a ledger row: the T3 record the S-7 design asks of task S7-9
  (`.port/parity-closure/S7-ALLOCATOR-DESIGN.md` §3 "T3 call shape", §7 item 9). It consolidates the T3
  residuals recorded per family in N272, N274, N315, N320, N325, N331, N332 and N334, and adds the ones
  those records do not carry. Routing (T1) and failure semantics (T2) are closed (`DEFERRALS.md` F-008
  Tier 2). Nothing below routes an allocation to the wrong allocator or turns a reference `try` into an
  abort.
- **severity**: low. **kind**: allocator call shape (the sizes, counts and order of the calls a C
  `RusttyAllocator` sees), against pin 20abdb50a.
- **what "C-observable" means here**: a C embedder's allocator callbacks see a different number, size,
  alignment or order of `alloc`/`resize`/`remap`/`free` calls from the reference's, so a failing-after-n
  allocator fails at a different call. Output bytes, results, and live bytes after `free` are the same in
  every item unless the item says otherwise.
- **C-observable residuals** (each: port site, reason):
  1. **Render-state cell storage (USER-ACCEPTED 2026-09-26)** (N315, review S7-5 F2):
     `crates/rustty-vt/src/render.rs:400` (`CellList`) and `:570` (`set_capacity`).
     - A row's cells are three allocations (the raw, grapheme-span and style columns), not one
       `MultiArrayList` block. So each row growth makes three alloc calls (and later three frees) instead
       of one: a 24-row first update makes 73 allocations where the reference makes 25. With overscan
       every captured row counts (P7-W3 T1): the same update with `{above 4, below 1}` scrolled into
       history builds 29 rows, 88 allocations where the reference makes 30 (N315).
     - The allocator sees per-column sizes and alignments instead of `capacityInBytes(cap)` at `big_align`.
     - If the second or third column is refused, the port makes an extra alloc/free pair.
     - Reason: the faithful single block was implemented and measured at +3.4% cycles on
       `screen-clone.render`, outside the +2% fence. The per-column lists measured +0.1%.
  2. **Render-state per-row arena** (N315 (2) and (3)): `render.rs:404` (`grapheme_buf`), and the
     `reset(.retain_capacity)` sites at `:588-591` and `:1830-1838`.
     - The reference's per-row `ArenaAllocator` (grapheme dupes, highlights) is, in the port, a retained
       grapheme buffer plus a retained highlight list on the same allocator.
     - The grapheme buffer grows by `try_reserve`, allocator_api2's amortized policy (item 7).
     - The steady state matches: once capacities are reached, neither allocates. The growth sequence
       differs.
     - Reason: a real arena would hand out grapheme slices that dangle for the stale slots a safe API
       exposes.
  3. **Search element sizes** (N325 (1)):
     - `crates/rustty-vt/src/highlight.rs:170`: `Chunk` is 24 bytes in an array of structs, against Zig's
       20 in a struct of arrays. So a clone of a one-chunk highlight is 17 x 24 bytes, not 17 x 20.
     - `highlight.rs:144`: `Flattened` is 40 bytes, against 32.
     - `search/screen.rs:469`: `matches()` returns 24-byte shallow copies, against 32.
     - Element COUNTS follow the reference (`init_capacity` uses the reference's sizes); byte sizes are
       Rust's. Reason: Rust layouts. No SoA `MultiArrayList` port was taken for these, since nothing measured
       a need for one.
  4. **Search and formatter write granularity** (N325 (2), N331):
     - The search's encoding buffer and the C API's allocating writers (`rustty_formatter_format_alloc`,
       `rustty_selection_format_alloc`) grow as the port formatter writes. Its 512-byte cell-run buffer
       mirrors the reference's, but the growth has not been verified call for call against the reference's
       `Writer` drains.
     - The continuation writers match exactly since S7-8b (first growth 129).
  5. **OSC kitty-color list** (N320):
     - Its element is Rust's size, so the first reservation, `growCapacity(1)`'s `cache_line / @sizeOf(T)`,
       differs from Zig's.
     - `SegmentedList::shrink_capacity` (`crates/rustty-datastruct/src/segmented_list.rs:433`) always takes
       the "resize refused" arm. No OSC path calls it.
  6. **Kitty graphics payload hand-over** (N274): `crates/rustty-vt/src/kitty/graphics_command.rs:384-429`
     (`decode_data`).
     - The reference's `toOwnedSlice(alloc)` shrinks the list to the decoded length (a `remap`, else
       alloc + copy + free).
     - The port hands the list over at its full capacity. So there is one call fewer, and the later `free`
       passes the capacity instead of the decoded length.
  7. **allocator_api2 amortized growth where a site has not switched to `growCapacity`**:
     - A caller-routed `AVec` grown with `try_reserve`/`try_extend_copy` follows RawVec's policy: a minimum
       non-zero capacity of 8 for 1-byte elements (4 up to 1 KiB), then `max(2 * cap, needed)`. Zig's
       `ArrayList` asks for `growCapacity(n) = n + n/2 + init_capacity`.
     - Pinned by `zig_list::tests::rawvec_growth_is_the_recorded_t3_residual`:
       - a first push of one byte: 8 bytes, against Zig's 129;
       - a 513-byte buffer grown by one byte: 1026, against 899.
     - Sites that still grow this way:
       - The stack-fallback writers' spill (`crates/rustty-vt/src/snapshot/io.rs:328`,
         `StackFallbackWriter`; PORT NOTE at `:354`). The first spill is one exact request at the crossing
         (513 bytes at N = 512), then RawVec growth. That covers the snapshot record scratch (N332) and the
         stream's eight response writers (N274). Zig's `Writer.Allocating` inside `stackFallback` can reach
         the fallback a little under N, and grows by `growCapacity`.
       - The render state's grapheme buffer (item 2).
       - The kitty graphics data, loading and frame lists (N274: the arenas became per-value `Bytes`).
     - Sites the S7 tasks moved to Zig's growth (`crates/rustty-vt/src/zig_list.rs`,
       `alloc::ArrayListUnmanaged`) match the reference: the render MultiArrayLists and ArrayLists (N315),
       the search lists (N325), the formatter pin map (N330), the continuation tracker (N274, since S7-4b),
       and the allocating writers (N331).
  8. **Hash maps**:
     - The kitty image and placement maps are `hashbrown` over the handle
       (`crates/rustty-vt/src/kitty/graphics_storage.rs:168`, `:170`). The reference's
       `AutoHashMapUnmanaged` is Zig's open-addressing table: one block holding the header, metadata, keys
       and values, at power-of-two capacity and 80% load. hashbrown allocates control bytes and buckets at
       7/8 load.
     - The zigstd `AutoArrayHashMapUnmanaged` (`crates/rustty-zigstd/src/array_hash_map.rs:91`) backs
       `PageList.tracked_pins`, the glyph glossary, and vt-c's `tracked_grid_refs`/`searches`. It is an
       `AVec` of entries plus a `hashbrown` index (`:62`). Zig's is a `MultiArrayList` plus an index
       header, built only above its linear-scan threshold (N272).
     - So one tracked pin costs 2 allocations (entries + index) where the reference makes 1. The routing
       tests pin the port's counts (`PageList.init`: pools 4 + pin set 2).
     - Call-for-call sizes against Zig's tables: UNVERIFIED (not measured).
  9. **`ret_addr`**:
     - Every C-arm call passes `ret_addr = 0` (`crates/rustty-vt/src/alloc.rs:672`, PORT NOTE W36-B06),
       where the reference forwards `@returnAddress()`.
     - Reason: stable Rust has no return-address intrinsic.
     - An embedder that logs or keys on `ret_addr` sees 0.
  10. **Windows `std.Io.Threaded`**:
      - On Windows, the reference's `ghostty_terminal_new` makes one more call through the caller's
        allocator, `alloc.create(std.Io.Threaded)` (`src/terminal/c/terminal.zig:73-80`), and frees it at
        `free` (`:87-95`, `:1864`).
      - The port makes no such call, because it has no `std.Io` model (`crates/rustty-vt-c/src/terminal.rs:128-167`;
        the TinyIo row of `.port/phase-4/NON_PORTS.md`).
      - CORRECTION of N334 and `DEFERRALS.md` F-008: both give the reason as "the port does not build for
        Windows". Since task W-1 (N342), rustty-vt-c builds and is tested on Windows, so this difference is
        now reachable there.
      - It is one allocation per terminal, not a routing gap.
  11. **Kitty file-medium canonical path buffers** (P7-W5 T2, `0b5446364`):
      `crates/rustty-vt/src/kitty/graphics_image.rs` `readFile` and `isPathInTempDir`.
      - The reference allocates each `std.fs.max_path_bytes` buffer through `alloc` (about 96 KiB on
        Windows), and so does the port: one alloc and one free of `MAX_PATH_BYTES` per buffer, the
        first before the open, the second only when the temp-directory prefix check reaches the real
        path fallback. An allocation failure is `error.OutOfMemory` in both.
      - The reference's `realPath` writes the canonical path straight into that buffer. The port's
        `realpath_of_handle` and `realpath` return it in a transient Rust-heap `Vec` (on Windows via a
        Rust-heap WTF-16 buffer too), which is copied into the buffer; a longer path is `NameTooLong`
        as in the reference. So the C allocator sees the same calls, and the Rust heap sees one more
        allocation per resolved path (listed under "Not C-observable" below).
- **Not C-observable** (listed so the inventory is complete):
  - **Rust-heap allocations** that the reference never makes, or makes inside caller-allocated storage
    (N334 has the details). Current lines:
    - rustty-vt: `formatter.rs:1723`, `:1749`, `:2170`; `string_map.rs:108`; `screen.rs:4659`
      (`Screen::selection_string_impl`'s output);
    - rustty-vt-c: the `Box<dyn ZigAllocator>` adapters at `terminal.rs:1620`, `:1958` and
      `render.rs:1354`, `:1549`; the kitty temporary-directory copies at `terminal.rs:3179`, `:3187`; the
      snapshot decoder's `Source`/`ReaderState` boxes at `snapshot.rs:323-324`;
    - N274's port-only transients: response strings, `format!` reports, map-key snapshots, and tmux
      notification copies (zlib's own `malloc` for inflate state is gone since P7-W5 T2: the kitty
      zlib decoder is wuffs, whose state and output are allocated through the caller's allocator, as
      the reference's are);
    - the kitty file media's canonical-path `Vec`s (item 11);
    - the per-dispatch `Box` of the borrowed OSC command (N320).

    The C allocator sees none of these, so a C embedder's memory accounting misses them.
  - **Render `string`/`link_cells`** (`render.rs:1580`, `:1637`) ignore their `alloc` and abort on OOM
    (N315's T1/T2 residual). They are not reachable from the C API, and every caller passes the Rust heap.
  - **`alloc::page_allocator()`**, the handle for the inspector's
    `pagePreservingState(std.heap.page_allocator)`, carries the PageList's VM tag on Darwin. The
    reference's page allocator is untagged (N274).
  - **The NULL allocator** is the Rust heap, whose `resize` succeeds only for an unchanged length (N270,
    N273). Zig's `c_allocator` also shrinks in place. This is internal to the library.
  - **Test-only differences**:
    - The setPwd/setTitle and stream-continuation failure tests refuse `remap`, to reproduce the reference
      `DebugAllocator`'s call sequence (N271, N277).
    - S7-3's pagelist fail points are port-local seams with no reference `tripwire.module`:
      `crates/rustty-vt/src/pagelist/grow.rs:19-46` (`GROW_TW`) and `pagelist/resize.rs:36-61`
      (`RESIZE_TW`), both for `createPage`. The reference reaches those errors with
      `std.testing.failing_allocator`.
    - It is UNVERIFIED whether their three tests could now run over a `FailingAllocator`, since the node
      pool has been on the handle since S7-3. The tests are `pagelist_erase_active_logs_when_regrow_fails`,
      `pagelist_resize_no_reflow_grow_rows_oom_restores_limits` and
      `pagelist_resize_no_reflow_grow_cols_oom_preserves_total_rows_and_limits`.
- **state**: RECORDED. The residuals are accepted as documented T3 deviations; item 1 is user-accepted,
  and item 10 is new since W-1. Any change is a future task per item; none is required for F-008's closure.

## N361 — strict provenance for page offsets; the remaining exposed-provenance sites

- **id**: N361. Not a ledger row: task S7-9 (Miri readiness of the S7 lane).
- **severity**: low. **kind**: Miri checking strength (no behaviour change).
- **found by**: the controller's S7 Miri lane. Every group warned "integer-to-pointer cast" at
  `crates/rustty-render-types/src/term_size.rs:303` (`OffsetBuf::init_offset`) and `:151` (`Offset::ptr`).
  An integer-to-pointer cast gives the pointer wildcard (exposed) provenance, so Miri may miss pointer bugs
  through any page offset.
- **fix**:
  - `IntFromBase` returns the base POINTER (`ptr_from_base`), and `int_from_base` is its address
    (`.addr()`).
  - `Offset::ptr` builds its result with `base_ptr.with_addr(addr)`, `OffsetBuf::init_offset` stores the
    base pointer, and `get_offset` subtracts `.addr()`s.
  - The addresses, the alignment assert and the codegen are unchanged: the reference's `@intFromPtr` +
    `@ptrFromInt` arithmetic stays address arithmetic.
  - The `&mut [T]` arm now uses `as_mut_ptr()`; the old `as_ptr()` went through a shared reborrow.
  - `rustty_vt::sys::GlobalRawAllocator::raw_alloc`'s zero-length sentinel is now
    `ptr::without_provenance_mut(align)` (it was `align as *mut u8`).
  - The vt-c `format_alloc` test's `@ptrFromInt(1)` sentinel is now `without_provenance_mut(1)`.
  - The S7-7 borrowed-command test compares addresses instead of casting an address back to a pointer.
- **verification**: `cargo +nightly miri test -p rustty-render-types`.
  - Before: "integer-to-pointer cast" at `term_size.rs:151`, and in `Offset_ptr_structural`.
  - After: no warning. It also passes under `MIRIFLAGS=-Zmiri-strict-provenance` (12/12).
  - This task did not run rustty-vt under Miri; that is the controller's lane.
- **remaining exposed-provenance sites (deliberate)**:
  - `crates/rustty-vt/src/hyperlink.rs:603-630` (`SetContext`) stores the page pointers as exposed
    addresses, so they survive the `&mut self.hyperlink_set` retags between the context assignment and its
    use.
  - `crates/rustty-vt-c/src/terminal.rs:715-737` (`wrapper_from_handler`) is the `@fieldParentPtr`
    recovery over the exposed wrapper allocation (memory `vtc-effects-trampoline-seam-d3`).
  - Each is its site's documented soundness mechanism, so Miri still warns once per group at
    `hyperlink.rs` when a test touches hyperlinks. Removing them needs a pointer derivation that does not
    pass through `&mut`; this task did not attempt that.
  - Test-only integer-to-pointer casts outside the S7 lane: the `crates/rustty-vt/src/bitmap_allocator.rs`
    tests (20 `*_addr as *const _` sites), and `pagelist/tests.rs:3339`, `:3395` (`node_addr as *…Node`,
    to move a pointer into `catch_unwind`).
- **risk to watch**: page pointers now carry the base's provenance instead of a wildcard. The next Miri run
  may therefore report Stacked Borrows violations that the wildcard hid, for example a pointer derived from
  `memory_ptr` used after a `&mut` over the whole page memory. Such a report is a real finding, not a
  regression from this change.
- **state**: CLOSED for `term_size.rs` (the warning is gone). The two deliberate sites stay (RECORDED).
- **reversal (P7-W2 T2, upstream `dd2edd760`, DECISIONS B1)**: the C API no longer hands out a
  zero-length sentinel address at all. `rustty_alloc(.., 0)` and the empty outputs of the three
  allocating exports are NULL, and borrowed empty strings point at the real `rustty_lib::EMPTY` static,
  so no provenance-free sentinel crosses the ABI. The selection test's `without_provenance_mut(1)`
  (`@ptrFromInt(1)`) input sentinel is unchanged. The same task gave `rustty-lib`'s typed-helper
  `zero_len_sentinel` (`allocator.rs`) the `without_provenance_mut` treatment this finding gave
  `GlobalRawAllocator`: it was an integer-to-pointer cast, which Miri flagged in "String.init empty
  output"; it is warning-free now. Miri (`formatter::`, `selection::`, `terminal::tests::continuation`,
  `allocator::`, rustty-lib `types::`): clean.

## N362 — three S7 tests were not Miri-ready: a Stacked Borrows violation in a test, a should_panic leak, an 18-hour test

- **id**: N362. Not a ledger row: task S7-9, from the controller's S7 Miri lane logs.
- **severity**: low. **kind**: test soundness and Miri tractability (no production change).
- **1. `stream_terminal::tests::response_writers_spill_through_gpa`**. Miri reported "not granted access
  to tag" at the `(*ptr).kitty_dnd` read.
  - The test took `let ptr: *mut Terminal = &mut t`, then called `t.vt_stream()`, whose `&mut self` retag
    invalidated `ptr`. The later `(*ptr).kitty_dnd` read is UB under Stacked Borrows.
  - Fixed: the serve closure takes the stream and reads the DnD slot through the stream's own
    `handler.terminal` pointer, which is derived from the `&mut t` that `vt_stream` took. Test-only.
- **2. `render::tests::render_state_checks_the_update_allocator`** (`#[should_panic]`).
  - The expected identity-assert panic unwound past the render state and the terminal. Neither has a
    `Drop` (their `deinit` frees), so Miri reported the leak.
  - Fixed with the pattern of `16ef8680`: catch the panic, `deinit` both, then `resume_unwind`.
  - Audit of the other S7 `should_panic` tests:
    - `terminal::tests::glyph_protocol_checks_the_glossary_allocator`: fixed by `16ef8680`;
    - `highlight::tests::flattened_deinit_checks_the_allocator`: the list is an `AVec`, whose `Drop` frees
      it during unwinding, so there is no leak;
    - `alloc::tests::debug_assert_same_fires_on_a_different_allocator`: builds nothing.
  - rustty-vt-c has no `should_panic` tests.
- **3. `pagelist::tests::pagelist_internals_route_through_the_allocator_handle`** ran 18 h under Miri
  before it was stopped.
  - Cause: `grow_cold_pages_for_test` makes a page cold with one `grow()` per row, about 600 grows, each
    running the O(page) integrity walk. The two clones also copied the whole first page row by row.
  - The page size itself has no test knob: `std_capacity` is fixed.
  - Fixed for both lanes, following the `pagelist_grow_prune_miri_tractable` precedent:
    - `make_first_page_cold_surgically_for_test` makes the cold page in constant time: it bumps
      `size.rows` to capacity, calls `grow()` once, and bumps the second page to the active height.
    - The clones start at the last history row. They still span both pages (pools sized for two), but copy
      25 rows.
    - The pin-remap step tracks a pin inside the cloned range (the viewport pin lies above it) and asserts
      that exactly one pin was remapped.
  - Every routing assertion is unchanged. Native debug runtime: 2.97 s before, 0.01 s after. Miri
    runtime: not measured by this task (controller lane).
- **not changed**:
  - `search::sliding_window::tests::sliding_window_decode_buffer_routes_through_the_allocator_handle`
    keeps its in-source MIRI EXCEPTION (multi-page history plus LZ4, the WS19 T6 class). It is not listed
    by name in `MIRI_EXCEPTIONS.md`.
  - The other S7 sweeps already shrink their corpus under `cfg(miri)`.
- **state**: CLOSED (task S7-9). Miri confirmation is the controller's lane.

## N365 — the CLI's writes to an unusable stdout/stderr succeeded in the port and fail in the reference

- **id**: N365. Not a ledger row: found by task W-4's Windows exe differential.
- **severity**: medium (the exit status of every action, and of "opening" the exe). **kind**: behavioural difference (library semantics).
- **finding**: the reference writes through `std.Io.File.stdout()`/`.stderr()`, whose writes fail with
  `error.NotOpenForWriting` on a missing or unusable handle. Every action `try`s its writes (or its flush), so it
  exits 1, and the no-action usage banner's `try stdout.print` makes `main` return `error.WriteFailed` (exit 1). The
  port wrote through Rust's `std::io::stdout()`/`stderr()`, which report success on `ERROR_INVALID_HANDLE`
  (Windows) and `EBADF` (Unix), so it exited 0. On Windows this is every "open" of the GUI-subsystem exe:
  MEASURED (Windows 11, reference exe at the pin, Debug and ReleaseFast): a shell start (`Start-Process`,
  `start /wait`) and a console start (`cmd /c ghostty.exe ...` in a console) of no-args and `+version` exit 1,
  with nothing on stderr in ReleaseFast (a Debug build dumps the error return trace). The port exited 0 in all
  four cases.
- **fix**: `crates/rustty-cli/src/io_file.rs` (Zig's `File.stdout()`/`.stderr()`: raw `WriteFile` / `write(2)`
  that return the OS error). Every action and `main_rustty.rs` now write through it, behind `BufWriter`s of the
  reference's buffer sizes (256/1024/2048/4096), so an error surfaces where the reference's buffered writer
  surfaces it (`try` propagates, `flush() catch {}` ignores). The banner's failure logs `error: WriteFailed`
  through the log function (off by default for the banner too, as in the reference) and exits 1.
- **tests**: `crates/rustty/tests/windows_launch.rs` (Windows): shell and console starts, no-args and `+version`,
  exit 1; positive control: the console start with `> file` exits 0 and writes the version. Negative control on
  Windows (io_file writing through `std::io::stdout()` again): both exit-1 tests fail with `right: 0`.
  `io_file::tests::a_write_to_an_unwritable_stdout_fails_where_std_reports_success` (Unix: fd 1 open read-only).
  Negative control on macOS: the same mutation, `RESULT ours_failed=false std_ok=true`.
- **closed descriptor (task W-4b, review W-4 F1)**: a CLOSED fd on Unix (`rustty +version >&-`). Rust's runtime
  reopens a closed fd 0-2 on `/dev/null` before `main`, so the write succeeded (MEASURED on macOS: exit 0); Zig's
  start code does not, and a Zig 0.16.0 program writing through `std.Io.File.stdout().writer(io, &buf)` gets
  `error.WriteFailed` (`NotOpenForWriting`, EBADF) and exits 1 (MEASURED by review W-4 with a Zig probe; not with a
  Ghostty build). Now matched: a C constructor in `io_file` (`.init_array` on Linux/FreeBSD/Android,
  `__DATA,__mod_init_func` on macOS), which runs before `lang_start`, records which of fd 1/2 were closed at exec,
  and a write to such a stream returns `EBADF`. MEASURED on macOS: `rustty +version >&-` and `rustty >&-` exit 1.
  Test `io_file::tests::a_write_to_a_stdout_closed_at_exec_fails_with_ebadf` (child with fd 1 closed before exec);
  negative control (the probe records nothing): `RESULT ebadf=false std_ok=true`. CORRECTION: this entry called
  the closed case "not fixable here".
- **state**: CLOSED (fixed, including the closed-descriptor case).

## N366 — `+show-config` compared the config against the bare struct defaults

- **id**: N366. Not a ledger row: found by task W-4.
- **severity**: medium (the command's default output was wrong on every platform). **kind**: port bug.
- **finding**: `FileFormatter.format` with `changed = true` builds the comparison base with `Config.default(alloc)`
  (`src/config/formatter.zig`), which adds the default keybinds, command-palette entries and URL link. The port used
  `Config::default()` (the struct defaults only), so `+show-config` listed every default `keybind` and
  `command-palette-entry` as a change. MEASURED on Windows against the reference exe: reference `command`,
  `working-directory`, `click-repeat-interval`, `auto-update-channel`; port the same plus every default keybind and every
  palette entry. Not Windows-specific.
- **fix**: `crates/rustty-config/src/formatter_file.rs` uses `Config::default_config()`.
- **test**: `formatter_file::tests::format_changed_compares_against_the_full_default_config` (a
  `default_config()` with `font-size = 42` prints exactly `font-size = 42\n`). Negative control: `Config::default()`
  again, the test fails listing the keybinds. The exe differential's `show-config` case now matches.
- **state**: CLOSED (fixed).

## N367 — startup log lines printed Rust `Debug` values where the reference prints Zig's `{}`

- **id**: N367. Not a ledger row: found by task W-4 (visible with `RUSTTY_LOG=stderr`).
- **severity**: low. **kind**: port bug (log text).
- **finding**: `global.zig:169-177` log `runtime={}`, `font_backend={}`, `renderer={}` and `main_ghostty.zig`
  `executing CLI action={}`: Zig renders an enum as `.` + tag name and a type as its `@typeName`. The port printed
  `runtime=None`, `font_backend=FreetypeWindows`, `renderer=Opengl`, `action=ListActions`. MEASURED against the
  reference exe: `.none`, `.freetype_windows`, `renderer.generic.Renderer(renderer.OpenGL)`, `.list-actions`.
  The same mapping was already fixed for `+version` (LEDGER S70-F4).
- **fix**: `crates/rustty-apprt/src/lib.rs` (`zig_runtime`, `zig_font_backend`, `zig_renderer`) and
  `crates/rustty/src/main_rustty.rs` (`.` + `Action::tag_name`).
- **test**: `bootstrap_tests::init_log_values_use_zig_rendering` (the three tables); the exe differential's
  `log-stderr-version` and `log-stderr-list-actions` cases compare the whole log, byte for byte, with the
  reference's (they failed before the fix, on exactly these lines).
- **task W-4b (review W-4 F8)**: the three lines are built by `runtime_line`/`font_backend_line`/`renderer_line`,
  which `init_with_action` logs; `bootstrap_tests::init_log_lines_are_the_references` asserts each line is one of
  the reference's `{}` renderings (negative control: `runtime_line` back on `{:?}` fails with `runtime=None`).
  `CLI action failed error={}` now prints `error.<Name>` (`rustty-cli` `Error::zig_name`: `Io` is
  `error.WriteFailed`, the writer error; `Parse`/`Config` reuse the existing renderings; the wrapped foreign error
  sets print their innermost variant name, UNVERIFIED set by set); test
  `rustty::tests::action_errors_render_as_zig_error_names`. CORRECTION: this entry left that line "not fixed".
- **state**: CLOSED (fixed).

## N368 — `+edit-config`'s Windows message had one blank line too many

- **id**: N368. Not a ledger row: found by task W-4.
- **severity**: low. **kind**: port bug (message text).
- **finding**: the reference's multi-line string literal ends with two empty `\\` lines and Zig adds no newline after
  the last one, so the message ends `path:\n\n`; the port wrote `path:\n\n\n`. MEASURED against the reference exe.
- **fix**: `crates/rustty-cli/src/edit_config.rs` `write_windows_unsupported`, together with UD342 (which adds the
  path after the blank line).
- **test**: `edit_config::tests::windows_message_names_the_config_path` (exact bytes).
- **state**: CLOSED (fixed).

## N369 — what "running Rustty on Windows" is at the pin, and how it is tested

- **id**: N369. Not a ledger row: task W-4 (the user: "The app seems to build on Windows, but opening it doesn't seem
  to work").
- **finding**: at the pin the Windows exe, Ghostty's and Rustty's, is the helper CLI (app runtime `.none`), a
  GUI-subsystem program with no window. "Opening" it shows nothing and it exits at once; typed at a prompt it shows
  nothing, because the shell does not wait for it and it cannot write to that console. Its output is visible only
  piped or redirected. This is the reference's behaviour (MEASURED with the reference exe, W-4 report); user-facing
  note `docs/windows.md`.
- **how the reference exe was obtained**: the pin never installs `ghostty.exe` with `.none`, and forcing it hits
  UD343. It was built on rwin with Zig 0.16.0 x86_64 (the aarch64-windows Zig 0.16.0 crashed with 0xC0000005 on
  every compile there, including a one-line `build-exe`), from the Mac's oracle checkout at `20abdb50a` with its
  fetched packages (no source from the internet), with `.port/tools/windows/ghostty-exe-at-pin.patch` and
  `-Dtarget=native-native-gnu` (the reference's own Windows CI job `build-libghostty-windows-gnu`), Debug and
  ReleaseFast.
- **harness**: `.port/tools/windows/exe-differential.ps1`: 50 CLI cases (every action the reference's
  `src/cli/ghostty.zig` registers, with valid/invalid/missing configs, bad flags, unknown and double actions,
  logging on), stdout/stderr/exit captured with process redirection, a fresh `LOCALAPPDATA` and home per case;
  plus the PE subsystem, the shell and console starts (exit code, no process left), and WER/Application Error
  events. `-Mode diff` (both exes), `-Mode capture` (goldens from the reference exe), `-Mode check` (CI, against
  `.port/tools/windows/golden/`, captured from the ReleaseFast reference exe). Final run (release vs ReleaseFast,
  and Debug vs Debug): every case matches except the recorded ones: UD22 (`+list-colors`), UD23 (`+list-themes`),
  UD340, UD341, UD342, and the R2 identity effects (the product-named palette entry sorts under G/R; the `class`
  default `com.mitchellh.ghostty`/`io.airely.rustty`).
- **also found**: the embedded config template is written with CRLF by a Windows build of the port (the checkout's
  `* text=auto`) and with LF by the reference exe built from the Mac's LF tree. A Windows checkout of Ghostty with
  `core.autocrlf=true` (the Windows default, and GitHub's runners) would embed CRLF too; the template is not in the
  matrix (it is a file side effect). UNVERIFIED: a reference build from a Windows checkout.
- **state**: CLOSED (the harness and goldens are committed and in the Windows CI leg).

## N370 — packaging manifests: the flatpak and snap dropped reference build flags; both flatpaks now verified offline

- **id**: N370. Not a ledger row: parity tail P-1 (packaging parity), 2026-09-27.
- **severity**: medium (a release flatpak shipped Debug-only resources). **kind**: build-flag translation.
- **Found by** diffing every reference packaging file, after the R2 rename, against Rustty's
  (`flatpak/*`, `snap/*`), at the pin `20abdb50a`.
- **Fixed**:
  - `flatpak/io.airely.rustty.yml`: `xtask resources` ran with its default `--optimize Debug`, so the
    release flatpak installed the terminfo source and termcap that only the reference's Debug build
    installs; it now passes `--optimize ReleaseFast` (the resource half of `-Doptimize=ReleaseFast`).
    `-Dstrip=false` is now `CARGO_PROFILE_RELEASE_DEBUG=true` + `CARGO_PROFILE_RELEASE_STRIP=false`, so
    flatpak-builder gets the symbols for the `.Debug` extension (measured: `io.airely.rustty.Debug`
    exported with content).
  - `flatpak/io.airely.rustty-debug.yml`: `--optimize Debug` (the reference's `-Doptimize=Debug`) in
    place of `--debug`. Its header claimed VERIFIED with the RELEASE manifest's commands; it had never
    been built. Now built and recorded.
  - `snap/snapcraft.yaml`: `grade: stable` (was `devel`); the dropped `#refresh-mode` comment;
    `-Dpatch-rpath=$ORIGIN/../usr/lib/<triplet>:/snap/core24/current/lib/<triplet>` as `xtask patchelf
    --patch-rpath` (the S63-F4 port); `--optimize ReleaseFast`; and `-fno-sys=gtk4-layer-shell` as a
    `meson` part building gtk4-layer-shell 1.1.0 (the flatpak's pinned release): core24 does not package
    it and rustty links it through pkg-config with no vendored copy. UNVERIFIED (V24).
  - `typos.toml`: the reference's `# TinyIo` / `tio` / `WRONLY` block had lost its comment and `WRONLY`.
- **Recorded, not changed**: `-Dcpu=baseline` (cargo's default target CPU is the baseline);
  `-fno-sys=oniguruma` (always vendored); the Zig module and pandoc in `dependencies.yml` (no Zig; man
  pages are generated in-process); `flatpak/zig-packages.json` (NON_PORTS G13 §6: the manifests build
  from the vendored `xtask dist` tarball with `CARGO_NET_OFFLINE`).
- **Verified (rvm, Fedora 44 aarch64, org.flatpak.Builder user-level, GNOME 50 SDK + rust-stable and
  llvm21 extensions)**: both manifests built offline from the `xtask dist` tarball, installed from a
  local repo and run. Release (rebuilt with `--disable-cache` after the xtask fixes): `+version` ->
  `Rustty 0.1.0-dev+0000000`, `build mode : .ReleaseFast`, `renderer.OpenGL`, GTK 4.22.5 runtime;
  `share/terminfo` has only the compiled database; no `share/rustty/webdata` (N373); the
  `/org/freedesktop/Flatpak/Development` host-command path is in the binary. Debug: `-debug`
  desktop/metainfo/D-Bus names, `rustty.terminfo` + `rustty.termcap` installed. On both,
  `desktop-file-validate` passes and `appstreamcli validate --no-net` passes (one pedantic note:
  `releases-info-missing`, since Rustty has no tagged releases). Of the plain `--target-os linux`
  resources: `systemd-analyze --user verify` passes on `app-io.airely.rustty.service`; the Nautilus
  extension byte-compiles; the Dolphin service menu fails `desktop-file-validate` (Type=Service keys)
  exactly as the reference's byte-identical `ghostty_dolphin.desktop` does: it is a KDE service menu,
  not an XDG application entry. The flatpak's Rust is the rust-stable extension (1.98.1), not the
  `rust-toolchain.toml` pin (1.93.1), as its manifest records.
- **state**: CLOSED (P-1), snap UNVERIFIED (V24).

## N371 — `xtask resources`: the Linux app id ignored the optimize mode

- **id**: N371. P-1. **severity**: medium. **kind**: behavioural difference.
- `GhosttyResources.zig:268-280` derives the desktop name (` (Debug)`) and the app id (`-debug`) from
  `-Doptimize` (Debug and ReleaseSafe are debug). `xtask resources` took them from a separate `--debug`
  flag, so every Debug run without `--debug` installed the release id and a ReleaseSafe build could not
  get the debug id at all.
- **Fixed**: `linux_app_is_debug(optimize, --debug)` in `xtask/src/main.rs`; `--debug` stays as an
  override. Callers now pass the mode: PACKAGING.md, `nix/package.nix` (mapped from its `optimize`),
  both flatpak manifests, the snap, and G-XTASK arms 2/3 (`--optimize ReleaseFast`).
- **Test**: `tests::linux_app_debug_id_follows_the_optimize_mode` (xtask). Negative control (the id
  taken from the flag alone): fails at the `Optimize::Debug` assertion.
- **state**: CLOSED (P-1).

## N372 — the CMake-built macOS lib-vt had rustc's 11.0 floor, not the reference's 13.0

- **id**: N372. P-1. **severity**: low. **kind**: build-option translation.
- `Config.zig:143-150` gives every macOS target `osVersionMin` 13.0 unless the caller set one;
  `xtask xcframework` and the metallib already applied it, but `CMakeLists.txt`'s cargo invocation did
  not, so `cmake --build` produced a dylib with `minos 11.0`.
- **Fixed**: on Apple the cargo command runs with `MACOSX_DEPLOYMENT_TARGET=13.0`, or the caller's own
  value when set (the reference's "if we have no minimum OS version"). Measured with `vtool
  -show-build`: `minos 13.0` through CMake; `minos 11.0` for the same build without the variable.
- **Not changed**: a plain `cargo build` still gets rustc's default (a `.cargo/config.toml` `[env]`
  would collide with the one `xtask dist` writes into the tarball); recorded here.
- **state**: CLOSED (P-1) for the CMake path.

## N373 — `-Demit-webdata` had no counterpart: every resources run installed the website data

- **id**: N373. P-1. **severity**: low. **kind**: missing build option.
- The reference installs `share/ghostty/webdata/{config,actions,commands}.mdx` only under
  `-Demit-webdata` ("Build the website data for the website.", default false; `build.zig:109`,
  `Config.zig:531-535`). `res_misc::gen_misc` always wrote them, so they shipped in the app bundle, the
  flatpak and the snap.
- **Fixed**: `xtask resources --emit-webdata <bool>` (default false) gates `gen_webdata`. G-XTASK arm 1
  asserts no webdata by default and arm 2 (`--emit-webdata true`) asserts all three `.mdx`; negative
  control (always emit): arm 1 RED "webdata emitted without --emit-webdata". Measured in the release
  flatpak: no `share/rustty/webdata`.
- **state**: CLOSED (P-1).

## N374 — `nix.yml` and `snap.yml` ported; `flatpak.yml` aligned with the reference

- **id**: N374. P-1. **severity**: low. **kind**: missing CI workflows.
- **Added** `.github/workflows/nix.yml` (the reference's `check-zig-cache-hash` in Cargo shape:
  `nix develop -c cargo metadata --locked`, plus its `Required Checks: Nix` aggregator) and
  `.github/workflows/snap.yml` (the reference's snapcraft build: udev + LXD cgroup workaround +
  `snapcore/action-build`, x86_64 and arm64, building the vendored tarball in-job as `flatpak.yml`
  does). Both UNVERIFIED and INERT behind `RUSTTY_NIX_ENABLED` / `RUSTTY_SNAP_ENABLED` (user deferral,
  V24). `xtask/tests/release_workflows.rs::deferred_tooling_workflows_are_gated_on_their_arming_variable`
  proves every job is gated; negative controls: a removed gate and an extra ungated job both fail it.
  `actionlint` is clean on all four touched workflows.
- **flatpak.yml**: the aarch64 leg (`ubuntu-24.04-arm`), the reference's `gnome-47` image, PR paths
  equal to push paths, `actions/checkout` v7.0.1 (also in `ci.yml`); the header records the merged
  smoke/publish shape, the missing repository guard (the merged smoke half runs on forks, as the
  reference's `build-flatpak` does) and the CLI in place of the flatpak-builder action. UNVERIFIED on
  GitHub (the arm64 container image is assumed multi-arch, as the reference uses it on arm64).
- **state**: CLOSED (P-1); execution debt V24.

## N375 — Nix, dotfile, example and macOS-plist parity fixes

- **id**: N375. P-1. **severity**: low. **kind**: content parity.
- **Fixed**:
  - `flake.nix`: `packages.default` was `librustty-vt` on every system; the reference's is the app on
    the platforms that build it (Linux). UNVERIFIED (V24).
  - `nix/devShell.nix`: `kaitai-struct-compiler` restored (the header's reason "G-KAITAI parses
    in-process" was wrong: `check_kaitai_snapshot.sh` needs the compiler), python `wasmtime` restored
    (`crates/rustty-vt-c/types-schema-verify.py` imports it), and the DROPPED list now names every
    dropped input (the vendored C libraries, the commented-out ones, the AFL++ macOS block).
  - `nix/tests.nix`: header said the VM tests are in `checks`; flake.nix wires them as `vmChecks`.
  - `.envrc`: the reference text (it had been reworded and watched `librustty-vt.nix` instead of
    `package.nix`/`wraptest.nix`); the reference's own wrong `wraptest` path is UD345.
  - `.gitignore`, `.prettierignore`, `.gitattributes`: the reference rules that had no counterpart
    (`*~`, `*.log` minus the committed `.port` logs, `.vscode/`, `.direnv/`, `.envrc.local`,
    `CMakeCache.txt`, `CMakeFiles/`, the qcow2 image and test outputs; `flake.lock`, `zig-out/`, the
    Swift example `.build/`; `*.zon`, the `*-cmin` corpora as generated, `crates/rustty-font/res`
    vendored). G-EOL stays GREEN.
  - `example/.gitignore`, `test/windows/.gitignore`: ported (were missing).
  - `macos/Rustty-Info.plist`: the rebrand-era `CFBundleName` key removed (the reference plist has
    none; `GENERATE_INFOPLIST_FILE` supplies it from the target name). Not rebuilt with xcodebuild by
    P-1; the ci.yml `macos-app` job builds it.
  - `example/c-vt-color-scheme/README.md`: the reference's description.
  - `Doxyfile` / `xtask/docker/lib-c-docs/Dockerfile`: the `doc/` output root is now recorded (the
    reference writes into `zig-out/share`, which rustty's packages copy wholesale); the Dockerfile
    comment claiming `zig-out/` was wrong. G-DOXYGEN GREEN.
- **P-1b (user decision 2026-09-27)**: `CLAUDE.md` is ported, a symlink to `AGENTS.md` (mode
  120000) exactly as the reference's `c09ade225`; `release-tip.yml`'s paths filter regains `!CLAUDE.md`
  and `!.envrc`. `.envrc` now carries the UD345 fix (R13, controller ruling) instead of the reference's
  wrong `nix/wraptest.nix` path. SUPERSEDED at P7-W10 T4 (2026-09-28, maintainer decision, DECISIONS
  "Maintainer decisions"): upstream itself deleted the symlink at `9d2d9acac` (20abdb50a..12752b2ac,
  R4 §L), so Rustty's copy is now removed too, for parity — `AGENTS.md` stays, and the
  `release-tip.yml` `!CLAUDE.md` filter entry is KEPT (upstream's own `release-tip.yml` still carries
  it at `12752b2ac`, naming a file that no longer exists there either). NON_PORTS G13 §6's row is
  updated to match.
- **Recorded, not changed**: `test/pkgconfig/test_librustty_vt.c` duplicating
  `nix/test-src/test_librustty_vt.c` (G-CCONSUMER uses one, the Nix tests the other);
  `macos/build-rustty-signed.sh` citing a `macos/DISTRIBUTION.md` that does not exist (Rustty-only
  script); `macos/MILESTONE.md:8` naming `dist/GhosttyKit.xcframework` (a dated history note).
- **state**: CLOSED (P-1).

## N376 — `test.yml` job by job: what `ci.yml` + `ci.sh` cover, and the jobs still without a counterpart

- **id**: N376. P-1 (read-only audit of the 53 jobs at the pin). **severity**: medium. **kind**: CI
  parity. **state**: OPEN, narrowed by parity tail CI-1 (2026-09-28): gaps 1-5, 7, 11-16 and 18-21
  are CLOSED (see "CI-1" below; the new jobs are statically checked and have not run on GitHub,
  VERIFICATION_DEBT V25); remaining: gap 6's wasm ABI-manifest step, gap 8, gap 9's
  `build-example-c-vt-build-info` passthru test, gap 10 and gap 17, each with its reason below.
  Previously: OPEN: the remaining items are CI legs P-1 could not run or verify; listed
  precisely for a follow-up batch.
- **Covered** (ported or translated, counterpart in brackets): `required` (ci.yml aggregator),
  `build-bench` (G-BENCH), `list-examples` + `build-examples-zig` (G-EXAMPLES, the 25 C/C++ examples;
  `zig-*` examples N/A), `build-examples-swift`, `build-cmake` + `test-lib-vt-pkgconfig` (G-CCONSUMER,
  which P-1 found RED on every host since S64-F5 and fixed), `build-flatpak` + `trigger-flatpak`
  (flatpak.yml), `build-libghostty-vt-windows`, `build-linux-libghostty`, `build-dist` (G-DIST),
  `build-macos` (macos-app job), `build-macos-freetype` (G-FONTBACKEND), `test`, `test-lib-vt`
  (+G-ABISCHEMA), `test-simd`, `test-macos`, `zig-fmt`, `pinact`/`prettier`/`swiftlint`/`typos`/
  `shellcheck`/`alejandra` (G-LINT), `blueprint-compiler`, `valgrind`, `build-examples-cmake`
  (G-EXAMPLES builds `c-vt-cmake` and `c-vt-cmake-static`; `c-vt-cmake-cross` is gap 3),
  `trigger-snap` (`snap.yml` triggers itself, gated off; its trigger is narrower, recorded in its
  header); recorded N/A: `build-libghostty-vt-freestanding` (no `no_std`; phase-6 W6),
  `build-example-zig-vt-macos-no-sdk`.
- **Counted by review P-1 (all 53 jobs now accounted for)**: `test-windows`
  (`zig build -Dapp-runtime=none test`) and `build-libghostty-windows-gnu` are gaps analysed in
  **N342** (the application tier does not compile for Windows; being addressed by W-2/W-4), not
  repeated below; `build-macos-xcode-27` (a second macOS app build on Xcode 27) has no counterpart and
  no record since NON_PORTS G13 §1 was superseded: gap 21. STALE as of P7-W10 T4 (2026-09-28): task
  W-2 later DID port both (once the app tier compiled on Windows, as the `windows` job's "Build/Test
  the workspace" and "Build the apprt-c library for the GNU ABI" steps); then upstream itself removed
  both jobs (`380778e3c`, 20abdb50a..12752b2ac, "libghostty-internal isn't really supposed to be used
  in Windows anyway"), so P7-W10 T4 removed the two ported steps again, for parity (DECISIONS R6).
  End state: NEITHER is a gap nor a Rustty-only surplus — upstream doesn't have them either. The vt-lib
  Windows test that used to ride along inside "Test the workspace" (a `cargo test --workspace` also
  runs `rustty-vt-c`'s own tests) got its own scoped "Test librustty-vt" step, matching
  `build-libghostty-vt-windows`'s unaffected `test-lib-vt` step.
- **Gaps (no counterpart or partial, no record)**:
  1. `skip` (path-filter fan-out; the fork/VOUCHED skip): needs a NON_PORTS record.
  2. `build-examples-cmake-windows` and the `c-vt-static` step of `build-libghostty-vt-windows`.
  3. `c-vt-cmake-cross` never builds on CI (no `x86_64-pc-windows-gnu` / `aarch64-unknown-linux-gnu`
     target or cross toolchain installed).
  4. `build-libghostty-vt`: no `aarch64-linux`, no `x86_64-linux-musl`.
  5. `build-libghostty-vt-android`: no NDK, no Android targets (S64-F9 is OPEN elsewhere).
  6. `build-libghostty-vt-wasm`: one profile, no wasm-opt/ReleaseSmall, no `test/wasm-alloc.mjs`, no
     schema check on wasm; `simd128` (N377).
  7. `build-libghostty-vt-features`: in effect NO CI counterpart. G-VTFEAT reads the oracle, which no
     CI leg has, so it reports BLOCKED on every leg (P-1 fixed it exiting 1 there; P-1b gave it a
     G-VTFEAT-SELF fixture self-test). Locally it builds the host only, with no wasm32 row and no
     per-feature `test --no-run`. A CI-capable form needs the expected export sets checked in, as
     A5's symbol manifest does.
  8. `build-libghostty-vt-macos`: the Mach-O dylib checks (`check-apple-libghostty-vt.nu`) have a
     subject now: NON_PORTS G7 §5.
  9. `build-linux` / `build-nix`: no arm64 Linux leg; `build-nix`'s debug build, `+version` mode greps,
     `nm` strip checks, the no-simd/debug lib variants and `build-example-c-vt-build-info` are not in
     `checks` (Nix deferred, V24).
  10. `build-dist-lib-vt`: no size assertion (the reference's 5 MB budget; Rustty's tarball is 43 MB).
  11. `build-snap`: the app is never compiled with `--features snap` (only the leaf crate).
  12. `test-kaitai`: G-KAITAI is BLOCKED on CI (no kaitai-struct-compiler installed).
  13. `test-gtk`: only gtk+wayland+x11 is compiled; the x11-only, wayland-only and neither
      combinations are not.
  14. `test-i18n`: there is no `-Di18n` override (N377).
  15. `test-sentry-linux`: `-p rustty --features sentry` is never built.
  16. `test-fuzz-libghostty`: CI has no cargo-fuzz, so the harness is never built there.
  17. `translations`: G-I18N's regenerate-and-msgcmp arm is deferred on a stale premise.
  18. `test-pkg-linux`: `rustty-wuffs` is built but not tested alone.
  19. `test-debian-13`: `xtask/docker/debian/Dockerfile` (the full-app build) is run by nothing.
  20. Workflow level: `on:` (the reference runs on every push to every branch) and `concurrency` (the
      reference never cancels a `main` run) differ, unrecorded.
  21. `build-macos-xcode-27`: no second-Xcode macOS build.
- **Reviewer's ranking (review P-1, by parity importance: can a shipped configuration or a
  user-visible behaviour regress unseen)**: 1. `test-windows` / `build-libghostty-windows-gnu` (N342);
  2. #11 `build-snap`; 3. #15 `test-sentry-linux`; 4. #13 `test-gtk` matrix; 5. #7 features (BLOCKED
  on CI); 6. #9 arm64 Linux / `build-nix` checks; 7. #4 aarch64-linux and musl; 8. #6 wasm;
  9. #16 fuzz build; 10. #12 kaitai; 11. #17 translations msgcmp; 12. #5 android (S64-F9); 13. #3
  `c-vt-cmake-cross`, #2 cmake-windows; 14. #8 macOS dylib checks; 15. #19 debian-13; 16. #18 wuffs
  alone, #14 `test-i18n`; 17. #10 tarball size; 18. #21 `build-macos-xcode-27`, #1 `skip`, #20
  `on:`/`concurrency`.
- **CI-1 (parity tail, 2026-09-28): gap by gap.** Each new `ci.yml` job is named after the reference
  job it ports, runs its steps with Cargo in place of `nix develop -c zig build`, has `needs: ci`
  (the reference's `needs: test`) and is in `required.needs`; runner mapping and the other common
  differences are in the section header in `ci.yml`. Proved by `xtask/tests/ci_workflow.rs` (11
  tests, each with an in-test negative control on a mutated copy of the real job) and by actionlint,
  prettier and G-LINT's actionlint/pinact arms. File-level controls, one at a time, restored: drop
  `test-debian-13` from `required.needs` (RED "`required` does not need `test-debian-13`"); NDK r29 ->
  r27 (RED "missing `ndk-version: r29`"); the SIMD-off leg's `--no-default-features` dropped (RED).
  None of the jobs has run on GitHub's runners: VERIFICATION_DEBT V25 names each.
  1. `skip`: CLOSED as a record, NON_PORTS G13 §6 (its only whole-run output keys on
     `.github/VOUCHED.td`, not carried; its path filters gate linters G-LINT runs on every push).
     `ci.yml`'s header says so.
  2. CLOSED before CI-1: the `windows-examples-cmake` job builds `c-vt-cmake` and
     `c-vt-cmake-static` on `windows-latest` (the latter is the `c-vt-static` step's subject).
  3. CLOSED: `build-examples-cmake-cross` (zig 0.16.0 via the reference's setup-zig pin,
     `x86_64-pc-windows-gnu`, the example's own `RusttyCrossCompiler.cmake`).
  4. CLOSED: `build-libghostty-vt`, the reference's six targets: the two macOS rows on
     `macos-latest`, `aarch64-unknown-linux-gnu` natively on `ubuntu-24.04-arm`, `x86_64-linux-gnu`,
     `x86_64-unknown-linux-musl` through `zig cc`, `x86_64-pc-windows-gnu` with MinGW-w64.
  5. CLOSED: `build-libghostty-vt-android`, N429's recipe.
  6. CLOSED except one step: `build-libghostty-vt-wasm` builds ReleaseFast (`build-vt-wasm.sh`) and
     ReleaseSmall (the same at `opt-level = "z"`), runs wasm-opt with the reference's flags, requires
     simd128 of both with `wasm-tools validate --features=-simd`, and runs `test/wasm-alloc.mjs` on
     both. REMAINING: "Validate WASM ABI manifest" (`test-lib-vt-schema -Dtarget=wasm32-freestanding`):
     G-ABISCHEMA loads the native dylib through ctypes and has no wasm path, so there is nothing to
     call yet; a gate change, not a workflow edit.
  7. CLOSED: `build-libghostty-vt-features`, the reference's compile-only matrix (its nine tokens in
     Cargo spelling, `-all` = `--no-default-features` keeping `simd`): a wasm32 build and a
     `cargo test --no-run` per configuration. G-VTFEAT's export-set comparison against the oracle is
     not a reference CI step and stays a local gate.
  8. REMAINING: the Apple dylib checks (`check-apple-libghostty-vt.nu`). Measured on the Mac: the
     `merge-w4` release `librustty_vt_c.dylib` is `arm64`, one `LC_BUILD_VERSION` (platform 1), no
     encryption command, 198 exports all `_rustty_`; an `aarch64-apple-ios` cdylib from
     `xtask xcframework-vt` is platform 2 with `LC_ENCRYPTION_INFO_64` (cryptoff 16384, cryptsize
     1605632, cryptid 0) and 198 `_rustty_` exports. So every arm would pass except the install
     name: Cargo's cdylib carries its absolute build path (`otool -D`), and only the CMake host
     install sets `@rpath/librustty-vt.dylib`, so the x86_64 and iOS rows have no installed artifact
     to check. Needs a per-target install step and the script's port; not a workflow edit.
  9. CLOSED except one test: `build-linux` (x86_64 and arm64), and `build-nix` (x86_64 and arm64)
     with the release and debug packages, their `+version` greps, the `nm` strip checks (N422) and
     the four librustty-vt variants. REMAINING: the reference's
     `build-example-c-vt-build-info` passthru test has no counterpart in `nix/librustty-vt.nix`.
  10. REMAINING: the 5 MB tarball budget. The lib-vt tarball is 43 MB because it vendors the crates
      it builds offline from; the assertion would fail by construction. A dist-content decision, not
      a workflow edit.
  11. CLOSED: `build-snap` (`cargo build -p rustty --features snap`).
  12. CLOSED before CI-1 by N402 (the `ci` job installs kaitai-struct-compiler on macOS; C-1 made
      G-KAITAI HARD).
  13. CLOSED: `test-gtk`, the four x11 x wayland rows (features plus RUSTTY_GTK_X11/WAYLAND),
      testing rustty-apprt and rustty and building the app.
  14. CLOSED: `test-i18n` (N421).
  15. CLOSED: `test-sentry-linux` (`-p rustty` with and without `--features sentry`).
  16. CLOSED: `test-fuzz-libghostty` (nightly, cargo-fuzz 0.13.2, `cargo +nightly fuzz build`;
      cargo-fuzz replaces AFL++, as FUZZ-INVENTORY.md records).
  17. REMAINING: G-I18N's regenerate-and-compare arm (`check_translations.sh`'s header defers it
      until `xtask update-translations` walks `crates/rustty-input`). The `translations` job's other
      arm, msgcmp, runs in `ci.sh`. A gate change, not a workflow edit.
  18. CLOSED: `test-pkg-linux` (`cargo test -p rustty-wuffs`).
  19. CLOSED: `test-debian-13` builds `xtask/docker/debian/Dockerfile` with `DISTRO_VERSION=13`
      from the checkout (the Dockerfile's documented input; the tarball's offline Debian 13 build is
      G-DIST's arm), 90-minute limit in place of the reference's 10.
  20. CLOSED: `on:` is the reference's (`push: {}`, `pull_request: {}`, `workflow_dispatch: {}`) and
      so is the concurrency group, which never cancels a `main` run.
  21. CLOSED: `build-macos-xcode-27` (`xcode-select -s /Applications/Xcode_27.app`, then
      `xtask xcframework`, the port of `-Demit-macos-app=false`); whether the hosted image has that
      Xcode is V25.

## N377 — the Zig build graph (`build.zig` + `src/build/*`) against Cargo + xtask: remaining options

- **id**: N377. P-1 (read-only audit). **severity**: low. **kind**: build-option parity. **state**:
  CLOSED by parity tail B-2 (2026-09-28), its `-Dpie` remainder by B-2b (2026-09-28): item 7 is N420,
  items 4 and 9 are N421, items 6 and 8 are N422 (`-Dpie` included), item 1 is N423, items 3 and 5 are
  N424, and both stale records below are corrected (N421, N423). The `-Dpie` default was B-2's remainder:
  measured with Zig 0.16 on the Mac, a default `zig build-exe` for `x86_64-linux-gnu` /
  `aarch64-linux-gnu` is `ET_EXEC` (non-PIE) and `-fPIE` gives `ET_DYN`, while rustc links Linux
  executables `-pie` (`ET_DYN`). B-2b ported it (`RUSTTY_PIE`, verified on the Linux VM; see N422).
  Previously: OPEN for the items below; everything else is ported, translated or recorded.
- **Covered**: every `b.step` (`run`, `run-valgrind`, `test`, `test-lib-vt{,-build,-schema}`,
  `test-valgrind`, `update-translations`, `dist`, `distcheck`), every `Config.zig` option except the
  ones below, every install artifact (terminfo/termcap per OS, shell integration, themes, completions,
  vim/nvim/bat, man + doc, locale, the Linux desktop/D-Bus/systemd/metainfo/Nautilus/Dolphin/icons,
  lib-vt shared/static/headers/.pc/cmake, xcframeworks, `ghostty-internal`), and the per-OS arms
  (S63-F5 Windows and S64-F9 Android are OPEN in other batches). The 88f57ee66..20abdb50a build changes
  (native-freestanding lib-vt) are recorded N/A in `.port/phase-6/assessments/W6-platform-tail.md`.
- **Fixed by P-1**: the app id follows `-Doptimize` (N371); `-Demit-webdata` (N373); the 13.0 macOS
  floor on the CMake path (N372); HACKING.md's Valgrind command now carries `run-valgrind`'s flags
  (`--num-callers=50 --suppressions=valgrind.supp --gen-suppressions=all`, `build.zig:345-352`);
  PACKAGING.md gains the reference's "WebAssembly (librustty-vt)" section in Cargo shape (commands
  run on macOS: the default and a `--no-default-features --features render-state` module build and
  patch; `+simd128` compiles) and its baseline-CPU bullet no longer calls rustc's default `native`.
- **Open**:
  1. `run` sets `GHOSTTY_RESOURCES_DIR=<prefix>/share/ghostty` (`build.zig:289-292`); `cargo run -p
     rustty` does not point at `zig-out/share/rustty`.
  2. (fixed by P-1, see above: HACKING.md's `run-valgrind` flags.)
  3. `dist` names the tarball, its prefix and `VERSION` after the full `cfg.version`
     (`GhosttyDist.zig:52-107`); `xtask dist` uses the bare workspace version. (A build from either
     tarball reports `X.Y.Z-dev+0000000`, measured in the flatpak, so the binary is unaffected.)
  4. `-Di18n` has no override and `xtask resources` compiles catalogs for every target.
  5. `-Dlib-version-string` has no counterpart (`nix/libghostty-vt.nix:79` uses it).
  6. The wasm `+simd128` default (`Config.zig:113-130`) is recorded only as "performance-only"
     (`docs/parity-audit/src-build.md:614`), which R8 does not accept for a dropped build switch.
  7. `-Dsimd=false` cannot be applied to the full app (`rustty-core` pins `simd`).
  8. `-Dstrip` / `-Dpie` defaults are recorded only as "cargo profile concerns" (`src-build.md:29,616`).
  9. `-Demit-docs` (`Config.zig:483-502`: on only when pandoc is found, or for a system package) has no
     override; `xtask resources` always writes the man pages and the doc tree (the rendering is
     in-process, so no pandoc is needed). Low severity.
- **Count**: 8 open (items 1 and 3-9; item 2 was fixed by P-1). The P-1 report's "7 open" predates
  item 9.
- **Reviewer's ranking (review P-1)**: 1. #7 `-Dsimd=false` for the full app; 2. #4 `-Di18n`
  override; 3. #6 wasm `simd128` default; 4. #8 `-Dstrip` / `-Dpie` defaults; 5. #1 `run`
  resources dir; 6. #9 `-Demit-docs`; 7. #3 dist naming from the full version; 8. #5
  `-Dlib-version-string`.
- **Stale records seen (not edited by P-1)**: `crates/rustty-build-config/src/lib.rs:66` names
  `app-runtime-*`/`renderer-*`/`i18n` features that do not exist; `xtask/src/main.rs:7` still says
  "PORT STATUS: scaffold".

## N378 — release workflows: residual differences from the reference

- **id**: N378. P-1 (read-only audit). **severity**: medium (one can overwrite the live tip appcast).
  **kind**: CI parity. **state**: CLOSED by parity tail CI-1 (2026-09-28) as far as a file edit
  goes: every residual below is ported (secrets referenced by name only); none has run, since the
  workflows need `RUSTTY_RELEASE_ENABLED` and the maintainer's secrets (VERIFICATION_DEBT V25).
- **P7-W10 T4 addendum (2026-09-28), R4 §G — action version bumps in the range `20abdb50a..12752b2ac`:**
  applied, with pinact-style SHA + comment: `ryand56/r2-upload-action` v1.4 `b801a390…` -> v1.5
  `33ebb7a4…` (7 sites: `publish-tag.yml`, `release-tag.yml`, 5x `release-tip.yml`);
  `softprops/action-gh-release` v3.0.2 `3d0d9888…` -> v3.0.3 `efb35369…` (5 sites, all
  `release-tip.yml`); `c-hive/gha-remove-artifacts` v1.4.0 `44fc7aca…` -> v1.8.0 `62c2fbea…` (1
  site, `clean-artifacts.yml`). All three release lanes stay inert behind `RUSTTY_RELEASE_ENABLED`
  (unverified by running, V25). N/A, recorded (no Rustty counterpart to bump): `nscloud-cache-action`
  (56 upstream sites — Rustty has no Namespace runner pool, per the existing "no nscloud-cache step"
  notes at `ci.yml:806`, `nix.yml:23`, `snap.yml:29`), `cachix-action` (55 sites — Rustty has no
  binary cache, per `nix.yml:25`; `cachix/install-nix-action` at `ci.yml:341,1161` and `nix.yml:67`
  is a DIFFERENT action, already pinned and unaffected by this bump), `docker/build-push-action`
  (Rustty's `test-debian-13` builds with a bare `docker build`, not this action) and
  `flatpak-github-actions/flatpak-builder` (Rustty's `build-flatpak` runs `flatpak-builder` as a CLI
  step, not through this action — N379/N-comment at `NEW-FINDINGS.md:7098`).
  Previously: OPEN: release workflows run only with maintainer secrets, so P-1 did not change them.
- **Fixed by P-1b**: the three `release-tip.yml` appcast steps require `github.ref_name == 'main'`
  again (test `tip_appcast_steps_run_only_on_main`, negative control); `release-tag.yml` `sentry-dsym` is
  `needs: [build-macos]` with no `if:`, as the reference; `!CLAUDE.md` / `!.envrc` restored in the
  tip paths filter; actionlint now runs in G-LINT. The rest below stays OPEN.
- `release-tip.yml` (was): the appcast upload no longer required `github.ref_name == 'main'`, so a
  dispatch from a branch of the canonical repository would overwrite the live tip appcast;
  `CFBundleShortVersionString $COMMIT`, `Delete :SUEnableAutomaticChecks` and `fetch-depth: 0` are
  missing; the `pr` dispatch input is dropped; the `source-tarball-lib-vt`,
  `build-lib-vt-xcframework` and `build-lib-vt-wasm` jobs have no R2 upload / URL echo; wasm ships
  one artifact where its comment claims two; the tip `sentry-dsym` job is omitted ("no Sentry
  project") while `release-tag.yml` ports it; its header still says two jobs are omitted that exist.
- `release-tag.yml`: the sentry-cli command changed; header lines 33-37 stale.
- `publish-tag.yml`: the staged `rustty-macos-universal-dsym.zip` is no longer checked.
- Picked up correctly from 88f57ee66..20abdb50a: milestone-action v3.2, Sparkle 2.9.6.
- **Fixed by CI-1** (`release-tip.yml` unless named): the `pr` dispatch input (`type: number`,
  optional) and the `release-urls-${{ inputs.pr || '0' }}` artifact it keys; `fetch-depth: 0` on
  `build-macos`'s checkout; `Set :CFBundleShortVersionString $RUSTTY_COMMIT` and `Delete
  :SUEnableAutomaticChecks` (the key is in `macos/Rustty-Info.plist`, false, as in the reference's
  plist); `source-tarball-lib-vt` and `build-lib-vt-xcframework` gain "Prep R2 Storage", "Upload to
  R2" (repository-pinned, `rustty-tip` bucket, the CF_R2_TIP_* secrets) and "Echo Release URLs";
  `build-lib-vt-wasm` now ships both tiers (`rustty-vt.wasm`, and `rustty-vt-small.wasm` built at
  `opt-level = "z"`), each wasm-opt'd, both required to need simd128 (`wasm-tools validate
  --features=-simd` must fail), minisigned, attached, uploaded to R2 and echoed; the tip `sentry-dsym`
  job (`needs: [setup, build-macos]`, downloads the dSYM from the tip host, `sentry-cli dif upload
  --project rustty --wait dsym.zip`); the header no longer lists the ported jobs as omitted. The
  debug-slow/fast variants stay omitted, as recorded in the header. `release-tag.yml`: `sentry-dsym`
  runs the reference's install (`curl -sL https://sentry.io/get-cli/ | bash`) and command (`sentry-cli
  dif upload --project rustty --wait rustty-macos-universal-dsym.zip`) on `ubuntu-latest`, and the
  stale header lines (the job listed as omitted, "rustty has no flake") are corrected.
  `publish-tag.yml` checks `rustty-macos-universal-dsym.zip` among the staged files again. Every
  secret-dependent step keeps this port's convention: a missing secret is a `::warning::`, not a
  failure. The Sentry project name `rustty` is an assumption for the maintainer to confirm.
- **Tests**: `release_workflows.rs` `tip_workflow_carries_the_n378_residuals` (16 required texts,
  and `sentry-dsym`'s needs; in-test controls: the short-version set, the delete, the `pr` input, the
  small wasm tier and the sentry job each changed in turn, all reported) and
  `tag_workflows_upload_and_check_the_dsym` (control: the publish check dropped). File-level control:
  the `Delete :SUEnableAutomaticChecks` line removed fails the first ("`build-macos` lacks ...").
  The existing gating tests still pass: every new job and step is behind `RUSTTY_RELEASE_ENABLED`
  (through `needs: [setup]`), and each publishing step pins the canonical repository.

## N379 — the fuzz package, `.github` script records and other residuals

- **id**: N379. P-1. **severity**: low. **kind**: records. **state**: **CLOSED** (G-1, 2026-09-28). (1) NON_PORTS: new `.port/phase-3/NON_PORTS.md` §G15 records every non-harness file of `test/fuzz-libghostty/` at the pin (`build.zig`/`build.zig.zon`, `pkg/afl++`, `replay-crashes.nu`, `corpus/sanitize-filenames.sh`, `.afl-tmin-temp-37266`, `AGENTS.md`/`README.md` as N/A or not ported with reasons; `.gitignore`/`.gitattributes` as ported). (2) Stale headers: `.github/scripts/rustty-tip` now says the flake and `nushell` exist and why the script stays bash; `check_xcframework.sh` names the reference job (`test.yml` `build-libghostty-vt-macos`, per-target dylib) instead of a release pipeline over the xcframework. (3) `run_fuzz_smoke.sh` seeds `fuzz_osc`/`fuzz_parser`/`fuzz_stream` from the mirrored `.port/phase-0/corpora/cvt/<name>-cmin` (byte-identical to the reference, `diff -rq` empty; 20/616/3271 files) with a scratch first corpus dir under `target/`, as `run-<name>` does (`build.zig:14-15`); `vt_diff` unchanged. Before this the three ran from an EMPTY corpus with `-runs=0`, i.e. essentially one empty input each. Measured: `FUZZ_SECS=5 run_fuzz_smoke.sh` GREEN over 4 targets; the stream log shows `seed corpus: files: 3271` and `INITED ... corp: 2810`. Negative control: `osc-cmin` moved away → `FAIL fuzz_osc — seed corpus missing or empty`, RED exit 1 (it first came out BLOCKED because no target ran; the BLOCKED arm now applies only when nothing failed). Was: **OPEN** (triage T-1, 2026-09-28). Left, unchanged: NON_PORTS records for the fuzz package's superseded pieces (`build.zig`/`build.zig.zon`, `replay-crashes.nu`, `corpus/sanitize-filenames.sh`, `.afl-tmin-temp-37266`, `pkg/afl++`, the package's `AGENTS.md`/`README.md`); the stale headers of `.github/scripts/rustty-tip` (still says no Nix flake/nushell) and G-XCF (`check_xcframework.sh:5`); and `run_fuzz_smoke.sh` seeding `cargo fuzz run` from the reference's `corpus/<name>-cmin` as `run-<name>` does. The P-1b split and the SC2155 lint are done (`9fd17d93`). Effort S, no long run; records and tooling only, not observable. Was: OPEN (records and one gate).
- `test/fuzz-libghostty/` -> `crates/rustty-vt/fuzz` (cargo-fuzz) is recorded only in
  `.port/phase-0/harness/FUZZ-INVENTORY.md`; `build.zig`/`build.zig.zon` -> `fuzz/Cargo.toml`,
  `replay-crashes.nu` and `corpus/sanitize-filenames.sh` (superseded by cargo-fuzz), the stray
  `.afl-tmin-temp-37266` (tool debris in the reference), `pkg/afl++` -> `libfuzzer-sys`, and the
  package's `AGENTS.md`/`README.md` have no NON_PORTS record. The reference's `run-<name>` step seeds
  afl-fuzz from `corpus/<name>-cmin`; `run_fuzz_smoke.sh` starts `cargo fuzz run` from an empty corpus.
- Governance non-port (user ruling), now listed in NON_PORTS G13 §6: `VOUCHED.td`, the 5
  `vouch-*.yml`, `DISCUSSION_TEMPLATE/vouch-request.yml`, `issue-unvouched-message`.
- Stale headers: `.github/scripts/rustty-tip:5-9` (says there is no Nix flake/nushell; both exist);
  G-XCF's header says the reference runs the Apple check over the xcframework in its release pipeline
  (it runs over per-target dylibs in `test.yml`).

- **P-1b addendum (2026-09-27)**: `ci.sh`'s `run_gate` executes a registry entry as a word-split argv,
  not through a shell, so the entries written `--self-test && <gate>` (G-XCF, G-P2) passed `&&` and the
  rest as arguments and ran ONLY the self-test; the real G-XCF and G-P2 never ran in `ci.sh`. Split
  into `G-XCF-SELF`/`G-XCF` and `G-P2-SELF`/`G-P2` (both real gates GREEN on macOS), and G-CCONSUMER
  got its own `G-CCONSUMER-SELF`. Also seen, not changed (another batch's file): G-LINT's shellcheck
  lane is RED on `.port/tools/gates/check_vt_alloc_vec_ban.sh:86` (SC2155).

## N380 — rustty-os and CLI diagnostics bypassed the log facade through `eprintln!`

- **id**: N380 (provisional: W-4's block N365-N369 was used up and N370-N379 are P-1's; the controller may
  renumber). Not a ledger row: found by review W-4 (out of scope there), fixed by task W-4b.
- **severity**: low. **kind**: port bug (diagnostics printed where the reference prints none).
- **finding**: the reference's `std.log.scoped(..)` calls go through `logFn`, whose stderr sink is off for a CLI
  action unless `GHOSTTY_LOG` turns it on. Fourteen port `log_*` helpers printed with a bare `eprintln!("[lvl] [scope] ...")`,
  which ignores the logging settings and the reference's `level(scope): message` form: rustty-os `mouse.rs`
  (`log_err`; on macOS every `+show-config` printed `[err] [os] NSEvent class not found. Can't get click
  interval.`), `file.rs` (`log_warn`, `log_debug`; a debug build printed `[debug] [os] file handle limit raised
  ...` on every run), `edit.rs`, `tempdir.rs`, `systemd.rs` (2), `flatpak.rs` (2), `open.rs`, `passwd.rs` (2), and
  rustty-cli `ssh.rs` (`log_warn`, beside the reference's own `Warning:` line) and `ssh_cache/entry.rs`
  (unscoped `std.log.warn`). Every scope matched the reference's already.
- **fix**: each helper calls the `log` facade with the reference's level and scope (`log::error!(target: LOG_SCOPE,
  ..)`, the empty target for the unscoped `std.log`), as `pty.rs` already did. The debug helpers lost their
  `cfg(debug_assertions)` gate: the facade applies the reference's Debug-only rule.
- **test**: `crates/rustty/tests/cli_log_facade.rs` (macOS): `rustty +show-config --no-pager` prints nothing on
  stderr; with `RUSTTY_LOG=stderr` it prints `error(os): NSEvent class not found. Can't get click interval.`.
  Negative control: `mouse.rs` `log_err` back on `eprintln!`, both tests fail.
- **not covered**: other crates' `eprintln!`/`println!` sites (renderer, apprt GTK, vt search thread, termio, ...)
  were not audited one by one here; the survey found no other `"[lvl] [scope]"` helper outside these files.
- **state**: CLOSED (fixed).

## N381 — POSIX `createMap`/`getAlloc`: the reference's last entry wins, the port's first

- **id**: N381. Not a ledger row: found by task W-5 while porting N340.
- **severity**: low (an `envp` with a duplicate name; only a C caller or `execve` can hand one over).
  **kind**: port difference (R1, macOS/Linux), recorded, not changed.
- **finding**: Zig 0.16's `putPosixBlock` calls `Map.put` for every entry (`std/process/Environ.zig:184-196`), and
  `put` replaces the value of an existing name (`:256-271`), so in `createMap` the LAST of two `K=` entries wins; and
  `getAlloc` goes through `createMap` (`:689-695`), so it returns the last one too. `getPosix` returns the FIRST
  (`:606-619`). The port's POSIX `create_map` keeps the first (`or_insert` / `contains_key`), and its `get_alloc` is
  `get_posix`, so both return the first; `environ_first_entry_wins` pins that. On Windows (task W-5) `create_map` is
  `putWindowsBlock`'s `putMove` (last wins), as in the reference.
- **why not changed**: task W-5's dispatch keeps macOS/Linux environment behaviour unchanged; the test above asserts
  the current behaviour deliberately.
- **state**: CLOSED (fixed under R1, controller ruling; task W-5b). POSIX `create_map` `put`s every entry (the last
  value wins; the first entry's key and position stay), `get_alloc` is `create_map().get(key)` on every target, and the
  map-based `contains`/`contains_unempty` are added (`Environ.zig:540-560`); `crates/rustty-os/src/edit.rs` now probes
  `VISUAL`/`EDITOR` with `contains_unempty` as `os/edit.zig:27,33` does. `get_posix` and the `*_constant` probes stay
  first-wins. Consequence: `Exec` builds the child's environment from `environ_map()`, so a child now inherits the last
  of two duplicates, as in the reference. Tests: `crates/rustty-global/tests/posix_duplicate_env.rs` re-runs the test
  binary through `posix_spawn` with the envp `[RUSTTY_N381_CHILD=1, K=first, E=, K=second, E=later]`
  (`std::process::Command` would drop the duplicate); the child asserts `get_posix` = `first`, `get_alloc`,
  `create_map`, `environ_map()` = `second`, `contains_unempty(E)` true and `contains_unempty_constant(E)` false; the
  parent asserts exit 0 and `1 passed`. Unit tests `environ_get_posix_takes_the_first_duplicate_and_the_map_the_last`,
  `environ_contains_unempty_reads_the_last_entry`, `edit::tests::duplicate_visual_entries_use_the_last_one`. Negative
  control (first-wins `create_map` restored), macOS and Linux (rvm): the child fails `getAlloc: last`
  (`left: "first" right: "second"`).

## N382 — an access violation in `rustty-xev`'s IOCP `async_notify` (once, rustty-core unit tests, Windows)

- **id**: N382. Not a ledger row: observed by task W-5 on rwin while loading the machine for N354.
- **severity**: medium if reachable outside tests (a use-after-free class crash); UNVERIFIED. **kind**: crash, cause
  not traced.
- **finding**: running `rustty-core`'s unit-test binary directly (built by `cargo test --no-run -p rustty-vt -p
  rustty-core -p rustty-os --target x86_64-pc-windows-msvc`, at W-5's 28c271fb) while `rustty-termio`'s suite and the
  rustty-vt unit tests ran at the same time, the process died with `0xC0000005` (exit -1073741819). Windows Error
  Reporting recorded fault offset `0x1e73a6` in the test exe; `llvm-symbolizer` (with the exe's PDB) resolves it to
  `rustty_xev::iocp::Loop::async_notify`, `crates/rustty-xev/src/iocp.rs:1333` (the read of `(*completion).op`), i.e.
  an `Async` was notified through a completion pointer that no longer pointed at a live `async_wait` completion.
  Which test, and whether the reference has the same race (N343: on IOCP the reference's `Async` is a value whose
  copies do not share a wakeup), was not established. 1 crash in 27 direct runs of the binary (the other 26, 16 of them
  with the rustty-vt load running, did not crash). The Windows workspace runs through cargo (baseline and final, W-5) did not
  crash.
- **next step**: run the binary under load with `--test-threads=1` and per-test logging to find the test; check the
  mailbox/`Async` teardown order in that test against the reference.
- **diagnosis (review W-5)**: a port TEST bug, not a product one. In
  `surface::paste_event_mutex_tests::paste_event_queues_while_holding_the_renderer_mutex_on_a_full_mailbox` the drainer
  thread waits on the surface mailbox's `Async` with a loop and completion on its own stack, deinitializes the loop and
  exits; the main thread then notifies once more. On IOCP `Async.wait` stores `waiter = {loop, c}` and every `notify`
  reads it (faithful to libxev `AsyncIOCP`, `watcher/async.zig:533-615`; the waiter must outlive every notify), so that
  notify read the freed thread stack. The reference has no counterpart test.
- **state**: CLOSED (test fixed, task W-5b, 7726df28): the drainer keeps its loop and wait completion until the main
  thread has set a `notified` flag right after its last `notify()`, then deinitializes. MEASURED on rwin with a 500 ms
  sleep injected before that notify: before the fix 5/5 runs crash with 0xC0000005, after it 10/10 pass. `iocp.rs` and
  `Async` unchanged.

## N383 — `cargo test -p rustty-core` alone on Windows fails two compression tests; the workspace run passes them

- **id**: N383. Not a ledger row: found by task W-5 on rwin.
- **severity**: low (test scoping). **kind**: test depends on cargo feature unification.
- **finding**: `renderer_seam::compression_tests::compression_idle_pass_compresses_scrollback` and
  `change_config_restarts_the_pass_only_on_disabled_to_enabled` assert `rustty_vt::compression_enabled()` ("this
  target supports strict reclamation"). On Windows that is true only when rustty-vt is built with
  `test-decommit-sim` (`crates/rustty-vt/src/mem.rs:175-188`; Windows has no strict reclamation arm), which
  rustty-vt-c's dev-dependency enables (N346), so it holds in `cargo test --workspace` and not when rustty-core is
  tested on its own: MEASURED 25/25 direct runs of a binary built with `-p rustty-vt -p rustty-core -p rustty-os`
  fail both (`assertion failed: latched.is_some()` at `renderer_seam.rs:2768`, and `:2675`). CI runs the workspace,
  so the CI leg is not affected.
- **fix direction**: give the two tests their own `test-decommit-sim` edge (a rustty-core dev-dependency feature), or
  scope them on the target (R14 form 1) with the workspace-only coverage stated.
- **fix** (task F-1): the first direction. `crates/rustty-core/Cargo.toml` has a
  `[target.'cfg(windows)'.dev-dependencies] rustty-vt = { workspace = true, features = ["test-decommit-sim"] }` edge:
  the tests then see `is_test` semantics on Windows as the reference's test binary does, without depending on the
  workspace's unification. Scoped to Windows because macOS and Linux reclaim natively, so their tests keep the OS
  path; a dev-dependency's features reach no production build (`resolver = "2"`). A sibling of the same kind:
  `rustty-inspector`'s `widgets::pagelist::ws7_tests::pagelist_compression_view_renders_live_numbers` and
  `pagelist_widget_draw_path_never_restores` also assert `compression_enabled()` and failed on Windows in a
  `-p`-scoped run that did not unify the feature (MEASURED on rwin, `pagelist.rs:2677` and `:2782`); it gets the same
  edge. No other crate outside rustty-vt asserts it (grep).
- **evidence** (rwin, `x86_64-pc-windows-msvc`): before, `cargo test -p rustty-core compression` fails both
  (`assertion failed: latched.is_some()`, `renderer_seam.rs:2772`), 2 passed / 2 failed; after, 4/4, then 25/25 direct
  runs of the unit-test binary (`compression_tests`, 3 tests each). `cargo test -p rustty-inspector` alone: 21/21 + 2
  (before: 19/21 in the multi-crate run above). macOS: `cargo test -p rustty-core compression` 4/4.
- **state**: CLOSED (fixed).

## N384 — `EnvMap` iterated in random order; Zig's map iterates in insertion order (the child's envp order)

- **id**: N384. Not a ledger row: review W-5 F4.
- **severity**: low. **kind**: port defect (R1), fixed.
- **finding**: `std.process.Environ.Map` is an `ArrayHashMap` (`std/process/Environ.zig:99-103`): insertion order,
  `put`/`putMove` keep an existing entry's position, `orderedRemove` shifts, `swapRemove` moves the last entry into
  the hole. `Command` builds the child's `envp` by iterating the map (`createNullDelimitedEnvMap`), so the order is what
  the child sees in `environ`. The port's map was a `HashMap` (random order, one `remove`), and its note said the order
  was not observable.
- **fix** (task W-5b, b4d5ce6c): `EnvMap` is backed by an `IndexMap` (`indexmap` 2, already in the graph): `insert` =
  `put` (position kept), `put_move` = `putMove` (key replaced in place), `remove` = `orderedRemove`, `swap_remove` =
  `swapRemove`, used by the one reference `swapRemove` caller (`GHOSTTY_BASH_ENV`, `shell_integration.zig:382`); every
  other port removal maps to an `orderedRemove`. Lookups borrow the key (`indexmap::Equivalent`), so they no longer
  allocate (review W-5 F3).
- **tests**: `environ::tests::env_map_keeps_the_array_hash_map_order`,
  `command::tests::create_null_delimited_env_map_keeps_the_map_order` (the envp order after `put`, `orderedRemove`,
  `swapRemove`). Negative controls, one at a time (macOS): `remove` as `swap_remove` and `put_move` as remove+insert
  each fail the first test; iteration reversed fails the envp test.
- **state**: CLOSED (fixed).

## N385 — `async_notify` forms `&Loop` on the notifying thread while the loop thread holds `&mut Loop` (IOCP)

- **id**: N385. Not a ledger row: review W-5 (informational).
- **severity**: low (aliasing-model UB, not a reproduced failure). **kind**: port soundness (Stacked Borrows).
- **finding**: `Async::notify` on IOCP (`crates/rustty-xev/src/iocp_watcher.rs:206-210`) calls `(*w.loop_).async_notify(w.c)` (the WASI watcher has the same call, but WASI
  runs one thread), and `iocp::Loop::async_notify` takes `&self`
  (`iocp.rs:1330`). The notifier is another thread, while the loop thread is inside `Loop::run(&mut self)`
  (`iocp.rs:319`), which holds a `&mut Loop` for the whole run. Creating the shared reborrow from the raw pointer
  conflicts with that live unique borrow under Stacked Borrows, even though `async_notify` only reads `iocp_handle`
  (a HANDLE that does not change after `init`) and writes the completion's atomic `wakeup`. The reference passes a
  plain `*Loop` (no aliasing model). Pre-existing, not a cause of N382. Not run under Miri (Miri has no IOCP).
- **fix sketch**: make it `pub unsafe fn async_notify(loop_: *const Loop, completion: *mut Completion)` and read the
  handle through the raw pointer (`unsafe { (*loop_).iocp_handle }` as a place read via `addr_of!`, or copy the handle
  into the `Async`'s waiter at `wait` time), with no `&Loop` formed on the notifying thread.
- **second site (found while fixing)**: from `wait` on, a notifier may run while the loop thread's
  `start_completion` (`iocp.rs`) holds `match &mut (*completion).op` for the just-submitted async completion; the
  notifier reads `op` (the discriminant and the `wakeup` atomic), which invalidates that `&mut` under Stacked Borrows.
- **evidence (before/after)**: `.port/phase-2/mirihar-n385/` (non-FFI harness; the loop thread holds its protected
  `&mut Loop` through one stable raw root, as termio does, and two notifications are forced into the two windows).
  `bash run-all.sh`, miri nightly 2026-07-22: shipped shape `loopref_mutop` UB under SB ("would remove [Unique] which
  is strongly protected") and TB ("foreign to the protected tag"); the fix sketch's raw by-value read
  (`rawread_*`) UB under SB, clean under TB (the `mirihar-xev-mpsc` `wakeup_copied` result again); handle copied
  but `&mut op` kept (`cached_mutop`) UB under SB; applied shape `cached_sharedop` clean under SB and TB, also with
  `-Zmiri-many-seeds=0..8`, and `cargo +nightly miri test` there passes under both models.
- **fix**: `AsyncIOCP.wait` copies the loop's port handle into the waiter on the loop thread (`AsyncWaiter::
  iocp_handle`, PORT-ADDED); `notify` calls `Loop::async_notify_handle(w.iocp_handle, w.c)`, the reference
  `async_notify` body with the handle by value, so the notifying thread touches only the completion.
  `Loop::async_notify(&self, c)` stays for loop-thread callers (`wait`'s notify-first case) and forwards.
  `start_completion` takes the `async_wait` arm (push to `asyncs`, `active += 1`, state `active`) through a shared
  borrow before the `&mut` match. Behaviour unchanged (R1). The reference has the same shape in Zig, where it is
  not UB and the handle never changes after `init`: no UD. The WASI watcher's same-thread
  `(*w.loop_).async_notify` is unchanged (`wasi_poll`'s `async_notify` writes the `Loop`'s own `wakeup`, and WASI
  runs one thread).
- **tests**: the three reference `AsyncTests` in `iocp_watcher.rs` were skip-stubs; they are live ports now
  (`iocp_watcher_async`, `_notify_first`, `_batches_multiple_notifications`), plus PORT-ADDED
  `iocp_watcher_async_notify_from_another_thread_wakes_running_loop` (another thread notifies while the loop blocks
  in `GetQueuedCompletionStatusEx`; if the callback has not run 5 s later the notifier posts a rescue to the port and
  the test fails on `rescued`). Windows VM (`rwin`, `--target x86_64-pc-windows-msvc`): `cargo test -p rustty-xev`
  3/3 (44 passed each), `cargo test -p rustty-termio` 3/3 (67 passed, 14 ignored; its unit tests do not reach a
  registered IOCP waiter, the `io_thread`/`live_shell` suites are `cfg(unix)`), clippy `-D warnings` for
  rustty-xev + rustty-termio clean. Negative control (Windows, one run): the waiter's copied handle replaced by null
  makes the cross-thread test fail after 5.07 s with "notify's post did not wake the loop; the rescue post did".
  rustty-core does not build for Windows (N342), so no core test runs there.
- **state**: CLOSED (fixed).

## N390 — librustty-vt's release log level was `.err`; Zig's default in every release mode is `.info`

- **id**: N390. Not a ledger row: task C-1 (G-REL, `sys::tests::log_facade_backend_reaches_the_c_sink` failed under
  `cargo test --release`).
- **severity**: medium. **kind**: port defect (R1), fixed.
- **finding**: `lib_vt.zig`'s `std_options` (pin, `:434-459`) sets no `log_level` for the `c_abi` arm, so
  `std.log.default_level` applies: `.debug` in Debug and `.info` in ReleaseSafe/ReleaseFast/ReleaseSmall (Zig 0.16
  `lib/std/log.zig:54-57`; the reference's `minimum_zig_version` is 0.16.0; Zig 0.15.2 has the same switch).
  `sys::install_log_backend` set `LevelFilter::Error` for non-debug native builds, on the claim
  (`DECISION-logging.md` IMPORTANT 4) that the default resolves to `.err` in ReleaseFast. So a release embedder's log
  callback never received the info and warn records the reference delivers. The wasm arm (`.err` + `noop` sink,
  `lib_vt.zig:446-455`) is unaffected. The published header says the same as the reference's
  (`include/rustty/vt/sys.h:177-181`, `RUSTTY_SYS_OPT_LOG`): "Release builds emit info and above".
- **fix** (task C-1): the release arm is `LevelFilter::Info`; the doc comment carries a CORRECTION note.
- **test**: `sys::tests::log_facade_backend_reaches_the_c_sink` now asserts the warn and info records reach the sink on
  both profiles and the debug record only with `debug_assertions`. Negative control (`--release`, the release arm set
  to `Warn`): fails, the info record is missing.
- **state**: CLOSED (fixed).

## N391 — the terminal-stream benchmark's handler pointer was invalidated by moving its box (Stacked Borrows)

- **id**: N391. Not a ledger row: task C-1 (G6, `rustty-bench` Miri lane).
- **severity**: medium (UB in a benchmark; the shipped terminal is not affected). **kind**: port soundness.
- **finding**: `TerminalStream::create` boxed the struct with the terminal inline, took
  `tptr = &mut b.terminal` for the stream's `Handler`, then returned the box. Returning (moving) the
  `Box<TerminalStream>` retags it Unique, which invalidates `tptr`; so does `setup`'s
  `self.terminal.full_reset()`. The next `Stream::next` wrote through the dead tag
  (`stream_terminal.rs:588`). Miri, G6 conditions: `trying to retag from <..> for Unique permission ...
  <..> was later invalidated ... by a Unique retag` at `terminal_stream.rs:188`. The reference
  (`TerminalStream.zig`) has the same self-reference; Zig has no aliasing model.
- **fix** (task C-1): the terminal is in its own heap allocation (`Box::into_raw`), held as the same raw
  pointer the handler holds; every access (`setup`, `destroy`, the test) goes through it, and `destroy`
  frees it after `stream.deinit()`.
- **test**: `terminal_stream::terminal_stream` under Miri (G6 conditions): UB before, pass after (the
  whole `rustty-bench` lane: 37 passed, 12 ignored). Native: both terminal-stream tests pass.
- **state**: CLOSED (fixed).

## N392 — `rustty-vaxis`'s `image_tier` test binary never ran under Miri (it did not compile there)

- **id**: N392. Not a ledger row: task C-1 (G6, `rustty-vaxis` lane).
- **severity**: low (test coverage; the finding it hid is in test code). **kind**: gate coverage.
- **finding**: `tests/image_tier.rs` gated `png_fixture` on `#[cfg(not(miri))]`, but its callers carry only
  `#[cfg_attr(miri, ignore)]`, so they still compile: `cargo miri test -p rustty-vaxis` failed with
  `E0425 cannot find function png_fixture` after the lib tests, and the binary's other tests never ran
  under Miri. Compiled, one of them hit a Stacked Borrows violation in the helper `img_draw_expect`,
  which took `&mut Screen` while the `Window` it also received held a raw pointer to that screen (the
  function-entry Unique retag invalidated the window's pointer before `draw` used it).
- **fix** (task C-1): the gate is removed (the fixture only calls `rustty-zigimg`, which compiles under
  Miri); `img_draw_expect` takes the window alone and reads the placement back through `win.screen`.
- **test**: `cargo +nightly miri test -p rustty-vaxis --test image_tier`: E0425, then UB; after: 19 passed,
  3 ignored. The whole lane: 157 passed, 12 ignored. Native: `image_tier` passes.
- **state**: CLOSED (fixed).

## N393 — the C `Style` writer stored eight bytes through a one-byte field pointer (provenance)

- **id**: N393. Not a ledger row: task C-1 (G6, `rustty-vt-c` lane).
- **severity**: medium (UB on every C-API style read; no observed miscompile). **kind**: port soundness.
- **finding**: `style.rs` `Style::write` ports `@as(*align(1) u64, @ptrCast(&out.bold)).* = bytes` as
  `(&mut out.bold as *mut bool as *mut u64).write_unaligned(bytes)`. A pointer made from `&mut out.bold`
  has provenance for that one byte, so the 8-byte store is out of bounds for it. Miri (G6):
  `attempting a write access using <..> at alloc..[0x39], but that tag does not exist` (the tag covers
  `[0x38..0x39]`), `style::tests::convert_style_with_colors`.
- **fix** (task C-1): the pointer is derived from the whole `Style` (`out as *mut Style as *mut u8`,
  offset by `offset_of!(Style, bold)`); the same eight bytes are written, and the existing const
  asserts still pin the eight fields as consecutive.
- **test**: `style::tests::convert_style_with_colors` under Miri: UB before, pass after. Native: the
  `style::` tests pass.
- **state**: CLOSED (fixed).

## N394 — `RenderState::update` held a shared row slice across `set_dirty` writes (Stacked Borrows)

- **id**: N394. Not a ledger row: task C-1 (G6, found through `rustty-vt-c`'s render tests).
- **severity**: medium (UB on the partial-redraw path of every render update with dirty rows).
  **kind**: port soundness.
- **finding**: `render.rs` (`RenderState::update`, the per-chunk dirty scan) formed
  `page_rows: &[page::Row]` once per chunk and used it for every `RowDirtyMask::match_group` scan, while
  the loop between scans writes `(*page_row).set_dirty(false)` through the raw base pointer. The
  write's function-entry Unique retag pops the shared slice's tag, so the next scan retagged from a
  dead tag. Miri (G6): `trying to retag from <..> for SharedReadOnly permission ... <..> was later
  invalidated ... by a Unique function-entry retag` at `render.rs:1288`
  (`render::tests::render_row_get_set_dirty`, `render::tests::render_update` in `rustty-vt-c`).
- **fix** (task C-1): the shared slice is formed afresh for each scan (`page_rows()`), from the same
  raw base pointer; the writes and the builder keep using raw element pointers.
- **test**: `rustty-vt-c` `render::tests::render_row_get_set_dirty` and `render::tests::render_update` under Miri: UB before, pass after.
  Native: the render tests pass.
- **state**: CLOSED (fixed).

## N395 — G4's leg B compares the port against c-vt goldens recorded at the phase-0 pin; a fresh port driver diverges on three

- **id**: N395. Not a ledger row: task C-1 (the full `ci.sh` run on `tail-c1`).
- **severity**: medium (a HARD gate that was green only through stale local binaries). **kind**: gate
  evidence / harness.
- **finding**: in a fresh worktree `G4` was BLOCKED (no `harness/bin/vt_driver_ref`/`vt_driver_port`:
  build products, not tracked). The main checkout's copies are dated 2026-08-10, so the ci run at
  `9fd17d93` compared an August port build against the goldens. Built from this branch
  (`build_port_driver.sh`; `build_ref_driver.sh` links the prebuilt `ghostty/zig-out`, dated 2026-08-28,
  before the pin), leg A (12 fixtures) is byte-exact, and leg B diverges on `c-vt-effects`,
  `c-vt-formatter` and `c-vt-paste`. The port's new lines are the ones the reference's example sources
  print at the pin 20abdb50a (checked by their `printf` formats — `Formatted output:\n`,
  `  clipboard read (location=%d, mimes=%zu)`, `  -> pty (%zu bytes): ` — not by running the reference
  examples, so byte equality is UNVERIFIED; `c-vt-paste/src/main.c` changed upstream on 2026-08-23),
  but `.port/phase-0/corpora/cvt/*.golden` were recorded from the reference examples at the phase-0 pin
  (2026-06-21) and never regenerated. `git diff 4749c4e93 20abdb50a -- example/` also changes
  `c-vt-grid-ref-tracked`, `c-vt-grid-traverse`, `c-vt-kitty-graphics`, `c-vt-render`, both
  `c-vt-selection*`, `c-vt-stream`, `c-vt-static` and `cpp-vt-stream` (these still match their old
  goldens), and adds `c-vt-search` and `c-vt-snapshot`, which the corpus does not cover.
- **also fixed** (task C-1): `build_port_driver.sh` defaulted `RUSTTY_ROOT` to the main checkout, so from a
  worktree it built and linked the MAIN checkout's `rustty-vt-c` (into the main checkout's `target/`).
  It now derives the root from its own location.
- **fixed** (task C-1): libghostty-vt was built at the pin outside the oracle tree
  (`zig build -Demit-lib-vt=true -Doptimize=ReleaseSafe --prefix /tmp/... --cache-dir /tmp/...`), and each
  reference example source at the pin (read at 20abdb50a, `example/<case>/src/main.c`) compiled with clang
  against it and run. 18 of the 21 stored goldens are byte-identical to that output; `c-vt-effects`,
  `c-vt-formatter` and `c-vt-paste` were re-recorded from it (golden, stdout, stderr, exit), and the port's
  output equals the new ones. `c-vt-paste` is not deterministic in the reference: the OSC 5522 clipboard-read
  password (`pw=<base64>`) differs on every run (measured, two runs). Leg B therefore masks exactly that
  token on both sides (`cvt_normalize` in `run_differential.sh`); its self-test proves a password-only
  difference compares equal and a one-byte difference next to it does not. The two examples the corpus did
  not cover, `c-vt-search` and `c-vt-snapshot` (deterministic, measured), were recorded and added to the
  port build and the corpus generator. G4: GREEN, 12 fixtures and 23 c-vt cases byte-exact.
- **state**: CLOSED (fixed).

## N400 — G-BLUEPRINT's first run: green; its description of `blueprint-compiler format` was wrong

- **id**: N400. Parity tail L-5 (tool-BLOCKED gates). **severity**: low. **kind**: gate accuracy. **state**:
  CLOSED (fixed).
- **run**: on the Fedora 44 VM (`rvm`), blueprint-compiler 0.20.4 from `/usr/bin` (already installed there),
  at 4994a515: `G-BLUEPRINT GREEN: 19 templates compile and are formatted.` Negative controls (in the VM clone,
  restored): a wrongly indented line in `1.2/close-confirmation-dialog.blp` makes the formatting arm RED; an appended
  `Gtk.Bogus nonsense {` makes the compile arm RED ("Unexpected tokens", line 13).
- **finding**: the gate and `nix/build-support/check-blueprints.sh` said `blueprint-compiler format` "rewrites in
  place and has no --check mode". Measured on 0.20.4 it is the opposite: without `-f/--fix` it prints the diff,
  says "1 file would be formatted" and exits 1, leaving the file untouched; only `--fix` rewrites. The gate was
  still correct (the non-zero exit made it RED), but it reported an unformatted template as "blueprint-compiler
  format failed", and its "fix with" hint (`format {} +`, no `--fix`) fixed nothing.
- **fix**: comments corrected in both scripts; the RED message names unformatted templates; the hint is
  `blueprint-compiler format --fix {} +` (verified: applying it to the mutated file restores the original byte for
  byte and the gate goes green). The copy-then-diff arm is kept as a second net for a compiler that rewrites.
  What the gate checks is unchanged.

## N401 — G-GTKFEAT's first run since task-18: green, and its gated-test floor had drifted 53 below the measurement

- **id**: N401. Parity tail L-5. **severity**: low. **kind**: gate anti-vacuity. **state**: CLOSED (fixed).
- **run**: on `rvm` (gtk4 4.22.4, wayland-client 1.25.0, libX11 1.8.13, libadwaita 1.9.2; all dev packages
  already installed), at 4994a515: clippy `-D warnings` clean; 253 tests listed with `--features
  gtk,wayland,x11`, 33 without, so 220 gated (wayland_* 3, x11_* 4). GREEN, about 5 minutes cold.
- **finding**: the floor was 167, so up to 53 gated tests could stop compiling without the gate noticing (the same
  drift task-18 found at 141 -> 167).
- **fix**: `GTK_DELTA_FLOOR=220`, dated note in the script. Negative controls (VM clone, restored): an `unused_mut`
  in `src/gtk/winproto/x11.rs` plus deleting `tests/gtk_x11_winproto.rs` gives RED on all three arms (clippy:
  `variable does not need to be mutable`; delta 219 < 220, which the old floor could not see; x11_* 3 < 4).

## N402 — G-KAITAI: green on macOS with Homebrew's compiler; CI never installed one

- **id**: N402. Parity tail L-5. **severity**: low. **kind**: CI parity (N376 gap 12). **state**: CLOSED (fixed;
  the CI step itself is UNVERIFIED until a CI run).
- **run**: on the Mac, `brew install kaitai-struct-compiler` (0.11, pulls openjdk 27); the gate provisioned its
  cached `kaitaistruct` venv itself. `G-KAITAI PASS: 22/22 snapshot goldens validate against snapshot.ksy`.
  kaitai-struct-compiler prints one style warning (`cell_count` could be `num_cells`) for the `.ksy`, which is
  the reference's file verbatim. Negative controls, one at a time, restored: the envelope magic of
  `complete-v1.hex` changed to `HHOSTSNP` gives `ValidationNotEqualError ... expected b'GHOSTSNP'` and FAIL; a
  mutation of an unconstrained byte (`grid-v1.hex` offset 8, 0x94 -> 0x95) passes, correctly: the schema accepts
  any value there (the Rust codec tests pin the exact bytes).
- **finding**: neither CI leg installs kaitai-struct-compiler, so G-KAITAI was BLOCKED (a warning) on every CI
  run, while the reference's `test-kaitai` job is required.
- **fix**: `.github/workflows/ci.yml`'s gate-tooling step installs `kaitai-struct-compiler` with Homebrew on the
  macOS leg (one word on the existing `brew install` line, and the step name). The gate stays PROGRESS in
  `ci.sh`; promoting it to HARD now that CI can run it is a controller decision.

## N403 — G-VALGRIND reported test failures as valgrind findings, and printed the wrong stack

- **id**: N403. Parity tail L-5. **severity**: low. **kind**: gate diagnostics. **state**: CLOSED (fixed).
- **run**: on `rvm` with the user-level valgrind 3.27.1 (N405), at 4994a515, the default set (oniguruma, wuffs,
  vt-c): 6 test binaries clean, GREEN in 24 minutes (23.5 of them rustty-vt-c's 553 lib tests).
- **findings** (from negative controls in `rustty-wuffs`, VM clone, restored):
  1. A deliberate definite leak (`mem::forget(vec![0xa5u8; 4096])`) turned the gate RED, correctly (memcheck:
     `ERROR SUMMARY: 1 errors`, the 4,096-byte record), but the excerpt printed the stack of loss record 1, a
     48-byte *possibly lost* std thread-local, not the leak: it grepped every `at`/`by` line.
  2. A plain failing test (`assert_eq!(1 + 1, 3)`) was reported as "valgrind finding": the classifier grepped for
     "definitely lost", which LEAK SUMMARY prints as `definitely lost: 0 bytes in 0 blocks` on any run that ends
     with unfreed memory. Measured with the old gate: `FAIL rustty-wuffs/rustty_wuffs (valgrind finding)`.
  3. The `--suppressions` WHY said `valgrind.supp` suppresses "ONLY dyld/libSystem frames". It is the reference's
     file: 233 entries naming GTK/GLib/GDK/fontconfig frames, Mesa DRI objects and DRD-only kinds (audited by
     script); none names a frame these crates run.
- **fix**: the classifier reads memcheck's `ERROR SUMMARY: N errors` (what `--error-exitcode` acts on); the excerpt
  prints only error records (stacks before HEAP SUMMARY, and `definitely lost` records after it); a test failure
  shows libtest's `FAILED`/`panicked at` lines instead of valgrind's tail; the WHY is corrected. Re-run of both
  controls: the leak shows the 4,096-byte record and `valgrind finding: 1 error(s)`; the failing test shows
  `panicked at crates/rustty-wuffs/src/lib.rs:283:9` and `test failure, not a valgrind finding`.

## N404 — G-VALGRIND covered 3 crates; rustty-font-shaper had 88 definite leaks, one of them a reference defect

- **id**: N404. Parity tail L-5. **severity**: medium (one production leak, UD350; the rest test-only). **kind**:
  CI parity (reference `test-valgrind` scope) + port defects. **state**: CLOSED for rustty-font-shaper and
  rustty-simd (fixed, now in the gate's default set); CLOSED (scope: by N413, L-6) for the remaining crates.
- **scope**: the reference's `valgrind` job runs `zig build test-valgrind` (`build.zig:411-422`): the WHOLE unit-test
  executable (`src/main.zig`) under `valgrind --leak-check=full --error-exitcode=1 --num-callers=50
  --suppressions=valgrind.supp --gen-suppressions=all`, with valgrind's default `--errors-for-leak-kinds`
  (definite and possible). G-VALGRIND ran three crates (rustty-oniguruma, rustty-wuffs, rustty-vt-c) at `definite`
  only. The `definite` narrowing is deliberate and measured: every Rust test binary here has a 48-byte *possibly
  lost* std thread handle (`std::thread::current::init_current`), so the default kinds would fail every binary on
  std itself.
- **found**: the first memcheck run of rustty-font-shaper (Linux, FreeType/HarfBuzz/Fontconfig) gave 88 definite
  leaks, ERROR SUMMARY 88. By cause:
  1. 84 from `LoadOptions`'s `Default` impl, which the port invented (the reference field `library` has no default):
     it called `Library::init()` for a placeholder that every `LoadOptions { library: lib, ..Default::default() }`
     overwrote, so each construction initialized a FreeType library and leaked it (21 sites, all tests; production
     `SharedGridSet::collection` sets every field). Fixed: `LoadOptions::new(library)` with the reference field
     defaults replaces `Default` (the library is required again); the 21 sites use it (2 of them in
     `rustty-renderer` test helpers). Test: `collection::tests::load_options_new_is_the_reference_field_defaults`.
  2. `SharedGrid::init` consumed the resolver and, on error, dropped it; `Collection` has no `Drop`, so the faces
     leaked (`shared_grid::tests::init_error`, whose PORT NOTE claimed the drop freed them). The reference leaves
     cleanup to the caller, and its own `SharedGridSet.ref` does not do it: UD350 (R13). Fixed in `init` (deinit on
     every error path). Tests: `init_error` asserts the face deinit per fail point;
     `shared_grid_set::tests::ref_whose_grid_init_fails_deinits_the_collection` (exact build + teardown counts).
     Negative controls: see UD350.
  3. Test-only: five `face::freetype` tests (S91-F9, UD190, UD344, N53 probes) never deinited their library, and
     one leaked its HarfBuzz user-data key box on purpose; the discovered-font helper's two skip returns did not
     deinit the library (the reference's `errdefer lib.deinit()` runs there);
     `collection_error_exit_deinits_collection` passed `&mut None` as the discoverer, so `collection` created a
     Fontconfig discoverer in a temporary that was never deinited (its comment said discovery was skipped). All
     fixed in the tests.
- **after**: rustty-font-shaper under memcheck: 131 passed, 0 errors (131 s, from about 7 minutes: the leaked
  FreeType inits were most of the time); rustty-simd: 42 passed, 0 errors (243 s). Both added to the gate's
  default crate set (net-new coverage), with rustty-xev (N406). `cargo test -p rustty-font-shaper`: Mac 139
  passed + 5 ignored, 3/3; Linux 131 passed + 4 ignored, 3/3.
- **correction (review L-6 F3)**: the scope gap below is closed: superseded by N410 (termio), N411/N412
  (renderer), N418 (vt lib) and N413 (whole scope). The bullets are L-5's survey as it stood.
- **still open (scope)**: memcheck survey of further crates on `rvm` at this lane's head (gate script, default
  `definite` kinds), recorded for a follow-up batch; none of these is in the default set:
  - clean: rustty-os (2 binaries), rustty-input, rustty-config (4 binaries).
  - rustty-termio: after N406, the lib binary still reports a `Box<Pipeline>` leaked by the test
    `exec::tests::gather_stage_reads_output_that_arrives_with_the_hangup` (262,240 bytes), a
    `Syscall param execve(argv[0]) points to unaddressable byte(s)` in the parent process, errors inside forked
    children (each child runs under memcheck and prints its own summary), and a run longer than 500 s under
    memcheck (killed by the timeout). Not analysed further.
  - rustty-renderer: 2,477 errors from 52 contexts. Leaks: the Linux test helpers' deliberately leaked
    `SharedGrid`s and FreeType libraries (`generic::linux_impl::tests::test_font_grid_and_size`, "leaked so the
    `*mut SharedGrid` stays valid") and screen sets; plus 10 `Conditional jump or move depends on uninitialised
    value(s)` contexts in the GL golden tests (`golden_renders_custom_shader_passthrough`,
    `golden_renders_cursor_fg_bg`), probably the llvmpipe readback (UNVERIFIED; `--track-origins=yes` not run).
  - rustty-vt: the 10 integration-test binaries are clean (`fuzz_corpus_replay` alone took 22 minutes). The lib
    binary (3,223 tests) was stopped after about an hour at roughly 1,400 tests, with no memcheck error up to
    then and one test failure caused by the gate's working directory (N407, fixed). A full run needs a lane with
    a few hours of VM time.
  - not run: rustty-core, rustty-apprt (+ GTK features), rustty-cli, rustty-font, rustty-crash,
    rustty-inspector, the -sys crates' own tests, and the rest of the workspace.

## N405 — how the tool-BLOCKED gates were run without root (reproduction recipe)

- **id**: N405. Parity tail L-5. **kind**: tooling record. **state**: CLOSED (recorded).
- **G-BLUEPRINT, G-GTKFEAT**: nothing to install on `rvm`; blueprint-compiler 0.20.4 (`/usr/bin`) and the gtk4,
  libadwaita, wayland and X11 development packages were already there. Run them on the VM (both are BLOCKED on
  macOS by design). CI's Linux leg installs the same with apt (`blueprint-compiler`, `libgtk-4-dev`,
  `libadwaita-1-dev`, `libgtk4-layer-shell-dev`).
- **G-VALGRIND**: no sudo on `rvm`. `dnf download --arch=aarch64 valgrind` (valgrind-3.27.1-1.fc44), then
  `rpm2cpio valgrind-*.rpm | cpio -idm` into a user dir, then a `valgrind` wrapper first on `PATH`:
  `VALGRIND_LIB="$HOME/<dir>/root/usr/libexec/valgrind" exec "$HOME/<dir>/root/usr/bin/valgrind" "$@"` (the
  launcher looks for its tools under `/usr/libexec` otherwise). Fedora's glibc needed no debuginfo package
  (`dnf download glibc-debuginfo` is not in the enabled repos, and was not needed). Kept on the VM for future
  lanes at `~/l5-tools/` (`bin/valgrind` wrapper, `root/` tree, `rpms/`). CI's Linux leg installs `valgrind
  libc6-dbg` with apt. Default set runtime on the VM: about 30 minutes, most of it rustty-vt-c.
- **G-KAITAI**: on the Mac, `brew install kaitai-struct-compiler` (0.11, pulls openjdk 27); the gate provisions
  `~/.cache/rustty-kaitai-venv` with `kaitaistruct` itself. On `rvm` (OpenJDK 25 is installed there) the
  kaitai.io release zip would work the same way; not needed, not done. CI: N402.
- **G-NIX**: not touched (deferred by the user).

## N406 — every Linux event loop leaked its boxed completion queue (`dynamic::Loop` never dropped its backend)

- **id**: N406. Parity tail L-5. **severity**: low (240 bytes per termio thread, i.e. per terminal surface, on
  Linux). **kind**: port defect (R1), fixed.
- **finding**: `rustty-xev`'s Linux `Loop` is the port of `dynamic.zig`'s untagged union; its arms are
  `ManuallyDrop<io_uring::Loop>` / `ManuallyDrop<epoll::Loop>` and it had no `Drop`, so after `deinit` (which
  closes the fds, the reference's whole `deinit`) the arm's Rust-owned fields were never dropped. The epoll loop's
  `thread_pool_completions` is a port-side `Box` (its field PORT NOTE: heap-rooted so the queue's
  self-referential stub survives reborrows), so each loop leaked it. Found by memcheck over rustty-termio
  (`Thread::init` -> `dynamic::Loop::init` -> `epoll::Loop::init`, 240 bytes, one per io-thread test).
- **fix**: `impl Drop for Loop` drops the live arm (named by the same backend global `deinit` reads). No backend
  loop implements `Drop`, so this adds no teardown after `deinit`, only the memory a directly-used backend loop
  frees anyway. The other `ManuallyDrop` unions in `dynamic.rs` (Completion, WriteQueue, WriteRequest, UdpState)
  showed no leak in the rustty-xev and rustty-termio memcheck runs and were not changed.
- **test**: `crates/rustty-xev/tests/dynamic_loop_drop.rs` (Linux; its own binary with a counting global
  allocator): an epoll loop holds heap while alive (240 bytes) and leaves 0 after `deinit` + drop. Negative
  control: without the `Drop` impl it fails, `left: 240, right: 0`. rustty-xev under memcheck: 3 binaries clean,
  now in G-VALGRIND's default set; `cargo test -p rustty-xev` and `-p rustty-termio` on Linux 3/3.
- **state**: CLOSED (fixed).

## N407 — G-VALGRIND ran test binaries from the repository root, not their package directory

- **id**: N407. Parity tail L-5. **severity**: low. **kind**: gate fidelity. **state**: CLOSED (fixed).
- **finding**: `cargo test` runs each test binary with the package directory as its working directory and
  `CARGO_MANIFEST_DIR` set; the gate ran them from the repository root with neither. rustty-vt's
  `kitty::graphics_image::tests::image_load_rgb_not_compressed_relative_regular_file` asserts the CWD and failed
  under the gate ("this test needs the cargo-set CWD": left `/home/gael/projects/l5`, right
  `.../crates/rustty-vt`), so any crate with such a test would be RED for a harness reason.
- **fix**: the binary list carries each artifact's package directory (`manifest_path` from cargo's JSON), and
  each binary runs in a subshell `cd`'d there with `CARGO_MANIFEST_DIR` set. Control (VM clone, restored): a
  temporary rustty-wuffs test asserting both passes under the new gate and fails under the old one (left
  `/home/gael/projects/l5`).

## N410 — rustty-termio under memcheck: an empty argv, a leaked Pipeline, and the 8-minute run

- **id**: N410. Parity tail L-6. **severity**: low (test-only). **kind**: test defects found by G-VALGRIND. **state**:
  CLOSED (fixed; the slow tests are faithful and unchanged).
- **finding** (L-5's N404 list, diagnosed on `rvm`, valgrind 3.27.1, at 7d8a101a):
  1. `command::tests::execvpe_latches_the_last_path_entry_errno` called `execve` with an EMPTY argv
     (`argv[0] == NULL`); memcheck: `Syscall param execve(argv[0]) points to unaddressable byte(s)`, 2 errors
     (one per PATH entry). The reference's `startPosix` always passes `argsZ`, whose first entry is the program.
  2. The "errors inside forked children" were those 2 errors again: a forked child inherits its parent's error
     count, and `--error-exitcode=1` turned the `_exit(0)` of
     `exec::child_raw_log_tests::log_err_child_raw_completes_in_a_child_forked_while_stderr_is_locked`'s child
     into exit status 1, so that test FAILED under memcheck (`ERROR SUMMARY: 2 errors from 1 contexts` in a child
     whose own records were clean). Not a harness artefact of forking: the other forked children print clean
     summaries (0 errors) and need no suppression; the reference's command line has the same default
     (`--trace-children=no`), so nothing was added for them.
  3. `exec::tests::gather_stage_reads_output_that_arrives_with_the_hangup` `Box::leak`ed its `Pipeline` so a hung
     gather stage could not hang the test: 262,240 bytes definitely lost on every run.
  4. The >500 s: the lib binary took 475 s under memcheck, 486 s of the earlier run in two tests,
     `termio_impl::scrollback_limit_handoff_tests::config_default_scrollback_limit_lines_is_unlimited` (249 s)
     and `..._config_scrollback_limit_lines_prunes_the_pagelist` (237 s); natively 5.3 s and 4.7 s in a debug
     build. Each feeds 4,760 linefeeds into an 80x3 terminal, and every `PageList::grow` runs
     `Page::assert_integrity` over the whole last page (sampled with `eu-stack`), which is the reference's
     `PageList.grow` -> `page.assertIntegrity()` under `slow_runtime_safety` (Debug). Faithful; not changed.
     The gate's shards spread them (N413).
- **fix**: (1) argv is `[program, NULL]`; (3) the test frees the box once the gather thread has returned
  (it stays leaked only in the hang case, which the `finished` assertion fails on).
- **negative controls** (VM clone, one mutant with both reverts, restored): memcheck over the three tests
  showed the `execve(argv[0])` error, the child test FAILED with the inherited `2 errors`, and the 262,240-byte
  definite leak; with the fixes: 3 passed, 0 errors.
- **after**: rustty-termio's 5 test binaries clean under memcheck (lib 92 passed + 6 ignored, 0 errors, 475 s);
  `cargo test -p rustty-termio` on the Mac 3/3 (118 passed, 4 ignored).

## N411 — rustty-renderer's test helpers leaked their font grids and terminals (2,477 memcheck errors)

- **id**: N411. Parity tail L-6. **severity**: low (test-only). **kind**: test defects (one R1 test-fidelity
  loss). **state**: CLOSED (fixed).
- **finding** (memcheck over the lib binary, `--track-origins=yes`, 52 contexts; 42 definite leaks, 10
  uninitialised-value contexts, the latter in N412):
  1. `generic::linux_impl::tests::test_font_grid_and_size` `Box::leak`ed every `SharedGrid` it built, with its
     FreeType library, faces and HarfBuzz shape-plan cache: 38 of the 42 leak records (25 grid boxes of about
     1.4 MB each, 8 FreeType libraries, their faces, one `hb_shape_plan`). No test freed them; the renderer
     never owns its grid (the app's `SharedGridSet` does).
  2. `link::tests` (all four `renderCellMap` tests) never called `t.deinit(alloc)` / `state.deinit(alloc)`:
     the reference's tests `defer` both (`renderer/link.zig:155,163,208,216,287,295,346,354`), and the port
     kept the first test's `// defer t.deinit(alloc);` comment without the call. An R1 test-fidelity loss
     (4 terminals).
  3. `surface_mailbox_routing::update_and_draw` built a `Terminal` and never freed it (2 terminals; found once
     the others were gone).
- **fix**: the grid is `Box::into_raw`'d; `free_test_font_grid` (grid `deinit`, the box, then the library, the
  order `shared_grid`'s own tests use) and `deinit_test_renderer` (`r.deinit()` then the grid) replace the 24
  test `r.deinit()` calls; the link tests end with the reference's `state.deinit` + `t.deinit`; `update_and_draw`
  deinits its terminal. The four skip arms that return before `Renderer::init` succeeded still leave the grid
  (they fail under `RUSTTY_REQUIRE_GL=1` before that matters).
- **after**: rustty-renderer under memcheck: 0 leaks, 0 errors with the N412 suppressions (lib 121 passed,
  98 s; `opengl_gate_parity` 3 passed); without them the 10 llvmpipe contexts remain and nothing else. Mac
  `cargo test -p rustty-renderer` 3/3 (121 passed, 1 ignored; the Linux-only helpers are not compiled there).
- **not changed**: the macOS twins (`generic.rs`'s `test_renderer` and `test_font_grid_and_size`) leak the same
  way; no leak detector runs on macOS (N404, G6's `detect_leaks=0`), so nothing reports them. Recorded, not
  fixed here.

## N412 — the renderer's 10 uninitialised-value reports are llvmpipe's, not the port's

- **id**: N412. Parity tail L-6. **severity**: none (tool noise). **kind**: gate fidelity (driver noise).
  **state**: CLOSED (suppressed, with evidence).
- **finding**: 2 contexts are inside LLVM's code generator (`AsmPrinter::emitFunctionEntryLabel`,
  `LowerConstantPool`) under `gallivm_jit_function`, origin a stack slot of Mesa's
  `lp_build_create_jit_compiler_for_module`; 8 are the golden tests' own comparisons of read-back pixels
  (`golden_renders_cursor_fg_bg`, `golden_renders_custom_shader_passthrough`), origin
  `lp_scene_new_data_block` (llvmpipe's scene memory), never a buffer the port uploaded.
  `--expensive-definedness-checks=yes` removes none.
- **evidence**: `.port/tools/valgrind-llvmpipe-probe/probe.c`, a C program with no Rustty code (EGL
  surfaceless, 64 instanced quads from fully written buffers, premultiplied-alpha blending as the renderer
  uses): every one of its 4,096 pixels has the expected value, and memcheck reports the same LLVM context and a
  conditional jump on its own pixel comparison (origin: JIT-compiled code). With blending off the pixel report
  goes away (2 runs each way): blending scales the destination by (1 - source alpha) = 0, and memcheck does not
  know that a product with a defined zero is defined. A single-triangle variant reproduces the LLVM context.
  Mesa 26.1.4 llvmpipe, LLVM 22.1.8, Fedora 44 aarch64. On GitHub's x86_64 Ubuntu image: UNVERIFIED (other Mesa
  and LLVM versions may surface the noise at other frames).
- **handling**: `.port/tools/gates/valgrind-llvmpipe.supp`, passed by G-VALGRIND after `valgrind.supp` (which
  stays the reference's file verbatim). The reference hides graphics-driver noise the same way, by naming the
  driver's frames ("Mesa", "Graphics Driver dri", "i965 addr4/addr8"); it runs no test that draws, so it has no
  llvmpipe entries. One entry ends at `fun:gallivm_jit_function` (every frame above it is LLVM or Mesa); five
  match only a conditional jump whose TOP frames are those two tests' own comparisons (the test body, its
  `Iterator::any` predicates, the slice `!=`, `Ord::max`). Each entry has its WHY (D6 audit clean).
- **correction (review L-6 F2, L-6b)**: those five entries matched every conditional jump whose top frame was the
  test body, renderer state included (the review hid a planted undefined `r.size.screen.width` at
  `assert_eq!(px.len(), ...)` with them). They are deleted. The two tests now declare exactly the read-back bytes
  defined, right after `read_rgba()`, with `rustty_zigstd::valgrind::memcheck::make_mem_defined` (a port of Zig
  0.16 `std.valgrind.memcheck.makeMemDefined`, request `ToolBase("MC") + 2` = 0x4d43_0002, a no-op natively);
  only the gallivm entry remains. Evidence in the L-6b report: the renderer under the gate is clean, and a value
  planted undefined in a test body is now reported.
- **negative control** (VM clone, restored): an uninitialised read planted in `px_at`, the helper those
  golden tests call, is still reported with both suppression files loaded (`ERROR SUMMARY: 212 errors from 3
  contexts (suppressed: 140 from 4)`), so the anchoring does not hide a function the test calls.

## N413 — G-VALGRIND now covers the whole workspace, sharded into its own CI job

- **id**: N413. Parity tail L-6. **severity**: medium (coverage). **kind**: CI parity (the reference's
  `test-valgrind` scope). **state**: CLOSED. Closes the scope gap N404 left open.
- **scope**: the default set is every workspace member except xtask (the port of `build.zig`, not in the
  reference's test executable), plus `rustty-apprt:gtk,wayland,x11` (GTK is the reference's Linux runtime, so its
  test executable there compiles the GTK apprt; `cargo test --workspace` does not) and `rustty-os:flatpak`: 62
  entries. A member missing from the list turns the gate RED (control: dropping `rustty-zf` from the list ->
  `RED: workspace members missing from DEFAULT_SCOPE ...: rustty-zf`).
- **gate changes** (`check_valgrind.sh`):
  - every process's `ERROR SUMMARY` decides, not only the test binary's, so an error in a forked child whose
    status the test ignores is a finding (control: a temporary rustty-wuffs test whose forked child leaks 4 KiB
    and `_exit(0)`s: the L-5 gate GREEN, this one RED "valgrind finding: 1 error(s)"); the error-record excerpt is
    tracked per process;
  - `--shard K/N` and `--jobs J`: every binary's `--list` is split round-robin into N*J parts, each its own
    memcheck process, J at a time; a binary that lists no tests (`harness = false`) runs whole in the one part its
    name hashes to. Verified: shards 1-3 of 3 with 2 jobs over rustty-cli, rustty-datastruct and rustty-apprt-c
    ran every listed test exactly once in total (121 + 89 + the apprt-c counts; the one N416 skip excepted);
    control: a planted 8 KiB leak shows up in its shard only, RED;
  - `cargo build -p <crate>` before `--no-run` (rustty-apprt-c's C-ABI smoke needs the cdylib, which ci.sh has
    from `cargo build --workspace`; the gate alone failed it: "missing cdylib");
  - named per-test skips, printed on every run (N416);
  - a build failure is RED (N417); `RUSTTY_VALGRIND_OWN_JOB` makes the gate report BLOCKED, naming the job that
    runs it.
  - (L-6b, review L-6 F1/F5/F6) `--self-test`, registered as G-VALGRIND-SELF, proves these arms against a fake
    valgrind and cargo on every leg; the summary line counts the named skips, and a skip whose test or binary no
    longer exists is RED; the required feature lanes are checked by name; xtask's
    `valgrind_job_matrix_covers_every_shard` ties ci.yml's shard matrix to the `--shard K/N` divisor.
- **CI** (`.github/workflows/ci.yml`): a `valgrind` job (4 matrix shards, `--shard K/4 --jobs 4`, 240-minute
  timeout, in `required.needs`) installs valgrind + libc6-dbg as the reference's job does, Mesa llvmpipe, and the
  GTK/GLib/blueprint/gettext build dependencies, and arms `RUSTTY_REQUIRE_GL`, `LIBGL_ALWAYS_SOFTWARE` and
  `RUSTTY_REQUIRE_FUZZ_CORPUS`; the `ci` job sets `RUSTTY_VALGRIND_OWN_JOB`, so its G-VALGRIND step is a BLOCKED
  warning naming that job. The reference also runs valgrind as a job of its own (30-minute timeout, one
  process). Not run on GitHub: UNVERIFIED there.
- **survey results** (VM, before fixes): clean at first run: every crate not named in N410, N411, N414-N416;
  the GTK lane's 36 binaries clean (most GTK tests are `#[ignore]`d for want of a display, as in every lane).
- **final run** (VM, at f4540223, exactly CI's four shards, `--shard K/4 --jobs 4`, two shards at a time on the
  VM's 8 cores, `RUSTTY_REQUIRE_GL=1 LIBGL_ALWAYS_SOFTWARE=1 RUSTTY_REQUIRE_FUZZ_CORPUS=1`): all four GREEN, 891
  memcheck processes (366 + 200 + 180 + 145), 0 errors. Memcheck phase per shard: 2,291 s, 3,014 s, 3,206 s,
  2,982 s (38-53 minutes; the longest single process 1,594 s, a rustty-vt lib part); the build before it about
  10 minutes cold. Total memcheck CPU for the scope is roughly 8.5 hours, three quarters of it rustty-vt's lib
  (its 3,223 tests take 578 s natively in a debug build). GitHub's 4-vCPU x86_64 runner: UNVERIFIED; the job's
  timeout is 240 minutes, about four times the VM's per-shard figure plus the build.

## N414 — two tests asserted they never run under Valgrind

- **id**: N414. Parity tail L-6. **severity**: low (test-only). **kind**: test defects found by G-VALGRIND.
  **state**: CLOSED (fixed).
- **finding**: `rustty-zigstd` `valgrind::tests::client_request_returns_default_natively` asserted that the
  `RunningOnValgrind` client request returns its default ("the test binary does not run under Valgrind"), and
  `rustty-core` `renderer_seam::cursor_blink_interval_tests::cursor_blink_interval_is_five_times_longer_under_valgrind`
  asserted `cursorBlinkInterval() == 600`. Under memcheck the request is answered (1) and the interval is 3000
  (the reference's `Thread.zig:735-748` behaviour, which the port reproduces): `left: 7, right: 1` and
  `left: 600, right: 3000`.
- **fix**: both tests tell the environments apart without the client request under test (`/proc/self/maps`
  names Valgrind's `vgpreload_*.so` in every program it runs) and assert the answer that environment calls for,
  so under G-VALGRIND they now also prove the request really reaches Valgrind. Natively (Mac 3/3, Linux) the
  assertions are the old ones.

## N415 — rustty-core's surface_visibility_kat leaked its Termio

- **id**: N415. Parity tail L-6. **severity**: low (test-only). **kind**: test defect. **state**: CLOSED (fixed).
- **finding**: the KAT's `TestSurface::drop` deinited only the Termio's terminal; the Exec backend's command
  (`Vec<CString>`) and environment (`EnvMap`), and the SPSC mailbox queue leaked: 4 definite leaks, 4,388 bytes.
- **fix**: `drop` moves the Termio out of the `MaybeUninit` surface and calls `Termio::deinit`, as
  `Surface.deinit` does (`self.io.deinit()`). Under memcheck: 0 errors; Mac 3/3.

## N416 — one rustty-cli test cannot run under Valgrind at all

- **id**: N416. Parity tail L-6. **severity**: none. **kind**: tool limitation. **state**: CLOSED (named skip).
- **finding**: `edit_config::exec_tests::exec_failure_report_prints_the_raw_command_bytes` makes `execve` fail
  with E2BIG in its own process (a 4 MiB command) to check the failure report. Valgrind cannot survive a failed
  execve of the program it runs: `EXEC FAILED: I can't recover from execve() failing, so I'm dying.`, and the
  whole rustty-cli lib binary died at that test.
- **handling**: G-VALGRIND's `VALGRIND_SKIPS` names it with the reason and prints it on every run; it still runs
  in every `cargo test` lane. The rest of the binary under memcheck: 118 passed, 3 ignored, 0 errors.

## N417 — G-VALGRIND reported a crate that failed to build as BLOCKED

- **id**: N417. Parity tail L-6. **severity**: low. **kind**: gate defect (R5). **state**: CLOSED (fixed).
- **finding**: the gate decided BLOCKED ("no test binary was run") before looking at failures, so when every
  crate in its set failed to build it exited 3, a warning, instead of RED. Control on the VM:
  `check_valgrind.sh rustty-no-such-crate` with the L-5 gate: `FAIL ... (build)` then `BLOCKED`, exit 3; with
  this one: `RED: rustty-no-such-crate(build)`, exit 1.

## N418 — rustty-vt's lib under memcheck: three test leaks, two of them lost `defer s.deinit()`s

- **id**: N418. Parity tail L-6. **severity**: low (test-only). **kind**: test defects (two R1 test-fidelity
  losses). **state**: CLOSED (fixed).
- **run**: G-VALGRIND over rustty-vt on `rvm` (5 parts at once, `--jobs 5`): all 3,223 lib tests (3,203 run, 20
  ignored as in every lane) and the integration binaries; 28 memcheck processes, 7,079 s wall, about 26,500 s of
  memcheck for the lib's five parts (3,905-5,981 s each). No invalid access, no uninitialised value; three
  definite leaks, all in tests:
  1. `pagelist::tests::pagelist_cold_compression_continues_after_an_incompressible_page` never deinited its
     PageList, so the compressed second page's encoded bytes (4,419 bytes, `CompressedPage::init`) leaked. The
     reference test has `defer s.deinit();` (`src/terminal/PageList.zig:9206-9210` at pin `20abdb50a`; the
     other test's is at `:9325-9330`); the port dropped it. `pagelist_compression_leaves_incompressible_pages_resident` dropped the same line (no
     heap leak there: its pages are mappings memcheck does not track).
  2. `page::tests::page_initBuf_rejects_unaligned_buffer` and `page_cloneBuf_rejects_unaligned_buffer` leaked
     their 12,288-byte buffers on purpose ("every user panics before returning").
- **fix**: both PageList tests end with the reference's `s.deinit()`; the page tests hold the buffer in a
  `PageUnalignedBuf` whose `Drop` frees it while the expected panic unwinds. Under memcheck the four tests:
  0 errors (before: the three records above). Test-only changes in `pagelist`/`page` tests: noted for a
  controller Miri pass (not run here, per the brief).
- **limit**: memcheck tracks heap blocks, not the page mappings (`mmap`) a PageList's pool is made of, so a
  test that forgets a PageList with no compressed page shows nothing here; that class stays with the Miri lane.

## N420 — `-Dsimd=false` could not be applied to the app, and no build of anything was SIMD-free

- **id**: N420. Parity tail B-2 (N377 item 7). **severity**: medium. **kind**: build-option parity.
  **state**: CLOSED (ported). Commits `860c6b98`, `ac8e5535` (packaging manifests), `40320c7e` (nix).
- **reference**: `Config.zig:238-249` (`-Dsimd`, default true unless wasm/freestanding). `cfg.simd` is the
  `build_options.simd` that `src/simd/*.zig` read (`comptime options.simd`: kernels or scalar
  fallbacks), the terminal module's `simd` (`GhosttyZig.zig:125-156`), and the gate on `addSimd`
  (`SharedDeps.zig:437`: highway, simdutf and the SIMD C++ are compiled and linked only when on). The
  reference CI's `test-simd` job runs `zig build test -Dsimd={true,false}`.
- **finding** (verified by reading `merge-w4`): `rustty-core` pinned `simd` on its `rustty-vt` edge and
  `rustty-bench` pinned it on both of its edges, so the app (and `rustty-apprt-c`, which links the bench)
  could not be built without it. Worse, `rustty-simd` hard-coded `SIMD = true` in all four modules and
  its build script compiled the C++ kernels unconditionally, with highway/simdutf as non-optional
  dependencies: `rustty-vt-c --no-default-features` and CMake's `RUSTTY_VT_SIMD=OFF` (documented as "a
  minimal static archive with no vendored C++ SIMD runtime") still linked and used them.
- **fix**: `rustty-simd` gains the `simd` feature (default-on for its own builds): the module consts are
  `crate::SIMD = cfg!(all(feature = "simd", not(target_arch = "wasm32")))`, the extern blocks and kernel
  calls are `cfg(all(feature = "simd", not(wasm32)))`, `build.rs` compiles nothing without it, and
  highway-sys/simdutf-sys are optional deps behind it. `rustty-vt/simd` chains it. `rustty-core`,
  `rustty-apprt`, `rustty-cli`, `rustty-bench`, `rustty-extra` and `rustty` get a default-on `simd` that
  threads it; their workspace edges (and `rustty-simd`'s) set `default-features = false`;
  `rustty-apprt-c`'s `simd` reaches all of them. No edge pins it. The flatpak (both), snap and
  `nix/package.nix` feature lists, which drop the defaults, name `simd`. The one R14 skip in rustty-simd
  (`decode_simd_matches_scalar`, the reference's `if (comptime !options.simd) return error.SkipZigTest`)
  is `#[cfg_attr(not(feature = "simd"), ignore)]`; `codepointWidth_matches_kernel` needs the kernel and is
  `cfg(feature = "simd")`, its dead `if !SIMD { return; }` deleted with G-SKIP's guard entry for it.
- **tests**: `rustty-core` `simd_option::simd_feature_reaches_the_terminal_and_the_simd_kernels` (both
  `rustty_vt::options::SIMD` and `rustty_simd::SIMD` equal the feature) and
  `tests/vt_build_options.rs` (rewritten: the flag follows the option; the manifest defaults it on,
  forwards it to both crates and does not pin it on the `rustty-vt` edge). Controls, one at a time,
  restored: re-pinning `simd` on core's `rustty-vt` edge fails the core test in the
  `--no-default-features` run (`rustty-vt's simd`: true vs false); dropping both forwards to
  `rustty-simd` fails it in the default run (`rustty-simd's simd`: false vs true).
- **measured** (macOS arm64, debug): `nm` of `librustty_apprt_c.a` — `--no-default-features`: 0
  `T ...simdutf`, 0 `T _rustty_simd_*`; default: 195 and 5. `librustty_vt_c.a` with
  `--no-default-features --features vt-features-default`: 0 and 0. The `rustty` binary:
  `--no-default-features` 0 SIMD symbols, default 384. `cargo tree -e features`: no highway/simdutf in the
  `--no-default-features` app graph. The app tier's tests with SIMD off (`cargo test --no-fail-fast -p
  rustty-core -p rustty-apprt -p rustty-cli -p rustty-bench -p rustty-apprt-c -p rustty
  --no-default-features`, after `cargo build -p rustty-apprt-c --no-default-features` for the smoke's
  cdylib): 332 passed, 0 failed, 7 ignored (the one extra ignore is apprt-c's R14
  `default_feature_set_resolves_terminal_simd_on`); `rustty-core` alone ×3: 73/73. `cargo test -p
  rustty-simd --no-default-features`: 40 passed, 1 ignored (the R14 skip above). One run of the six
  crates together failed `surface::paste_event_mutex_tests::paste_event_queues_while_holding_the_renderer_mutex_on_a_full_mailbox`
  ("the paste event never reached the IO mailbox", a 10 s timeout); it passed alone and in the three
  `rustty-core` runs and the later full run. Not investigated further: UNVERIFIED whether it is flaky on
  `merge-w4` too.
- **CI** (for N376 #`test-simd`, not wired: `ci.yml`/`ci.sh` belong to batch C-1): a leg running the
  command above (`--no-default-features`, plus `--features gtk,wayland,x11` for `-p rustty` on Linux).
  **Wired by parity tail CI-1 (2026-09-28):** `ci.yml` job `test-simd`, the reference's `simd:
  ["true", "false"]` matrix; the `false` row builds `rustty-apprt-c --no-default-features` and runs
  `cargo test --no-fail-fast -p rustty-simd -p rustty-core -p rustty-apprt -p rustty-cli -p
  rustty-bench -p rustty-apprt-c -p rustty --no-default-features --features
  rustty/gtk,rustty/wayland,rustty/x11`. Test `ci_workflow::test_simd_job_runs_the_simd_off_leg`
  (controls: the `false` row dropped, `rustty-core` dropped; file-level: `flags=()` in place of
  `--no-default-features`, RED). Not run on GitHub: VERIFICATION_DEBT V25.

## N421 — `-Di18n` and `-Demit-docs` had no override

- **id**: N421. Parity tail B-2 (N377 items 4 and 9). **severity**: low. **kind**: build-option parity.
  **state**: CLOSED (ported). Commit `2f075065`.
- **reference**: `-Di18n` (`Config.zig:263-271`): default macOS/iOS true, Linux/FreeBSD
  `isGnuLibC()`, else false; it is `build_config.i18n` (`os/i18n.zig` returns the msgid and binds nothing
  when off), gates `GhosttyI18n` (the `.mo` install, `build.zig:91`) and makes `update-translations` fail
  "cannot update translations when i18n is disabled" (`build.zig:449-455`). `-Demit-docs`
  (`Config.zig:483-502`): false when another artifact is emitted, true in system-package mode, else "only
  if we can find pandoc"; off, a Darwin target installs `GhosttyDocs.installDummy`'s
  `share/man/.placeholder` ("emit-docs not true so no man pages", `build.zig:95-103`).
- **fix**: `-Di18n` is the build-time variable `RUSTTY_I18N=true|false` (like `RUSTTY_VERSION_STRING` and
  `RUSTTY_GTK_X11`; a bool option with a per-target default is not a pair of additive features):
  `rustty-build-config`'s `build.rs` validates it (anything else panics) and
  `Config::from_options` resolves `I18N_OPTION` over `i18n_default` (`resolve_i18n`). `xtask resources`
  takes `--i18n <bool>`, defaulting to `RUSTTY_I18N` and then a per-target-os table (Linux layouts made
  off Linux assume glibc — `--target-os` names no libc; FreeBSD false as the reference's
  `isGnuLibC()` is), and installs `share/locale` only when on; `update-translations [--i18n <bool>]`
  refuses with the reference's message. `-Demit-docs` is `xtask resources --emit-docs <bool>`, default
  true: the pandoc probe stands for "the docs can be rendered", and the port's renderer is in-process
  (`rustty_extra::mdgen::render`), so every arm this step has is true. Off, no man pages or doc tree, and
  on a Darwin target the placeholder with the reference's text.
- **tests**: `rustty-build-config` `i18n_option_overrides_the_per_target_default` (every OS arm × unset /
  true / false) and `i18n_follows_the_build_option` (end to end through `option_env!`), run plain, with
  `RUSTTY_I18N=false` and with `RUSTTY_I18N=true`: all pass; `RUSTTY_I18N=yes` fails the build naming the
  variable. Control: `from_options` back on `i18n_default` fails `i18n_follows_the_build_option` under
  `RUSTTY_I18N=false`. xtask: `i18n_resolves_flag_then_env_then_target_default`,
  `emit_docs_defaults_on`, `update_translations_refuses_when_i18n_is_off`,
  `res_misc::docs_and_catalogs_follow_their_options_when_off` and `docs_are_emitted_when_on`. Controls,
  one at a time, restored: `gen_misc` always compiling catalogs fails the first ("no catalogs with
  -Di18n=false"); dropping the Darwin placeholder fails it (placeholder not found).
- **measured** (macOS): `RUSTTY_I18N=false cargo build -p rustty` → 0 `dgettext`/`bindtextdomain`
  references in the binary (the gettext calls are const-folded away); default and `RUSTTY_I18N=true` → 4.
  `xtask resources --i18n false --emit-docs false` → no `share/locale`, `share/man` holds only
  `.placeholder`, no `share/rustty/doc`; `RUSTTY_I18N=false xtask resources` → no `share/locale`, man pages
  present; default → `share/locale` with 34 entries.
- **by reading, not run**: with `-Di18n=false` on macOS the reference's own `canonicalizeLocale darwin`
  test would fail (`zh-Hans` comes back unchanged); its CI only builds that configuration, and so does the
  port's (`i18n` const-folds the same arm in `rustty-os`'s `canonicalizeLocale_darwin`). The macOS app's
  Xcode project references `zig-out/share/locale`, in the reference too, so an i18n-off macOS app build
  lacks that folder in both (UNVERIFIED what Xcode does then).
- **stale record corrected**: `crates/rustty-build-config/src/lib.rs`'s note naming `app-runtime-*` /
  `renderer-*` / `i18n` features.
- **CI** (N376 #14 `test-i18n`): builds with `RUSTTY_I18N=true` and `RUSTTY_I18N=false` (`cargo build -p
  rustty`), and `cargo test -p rustty-build-config` under each. **Wired by parity tail CI-1
  (2026-09-28):** `ci.yml` job `test-i18n`, matrix `i18n: ["true", "false"]` as the job env
  `RUSTTY_I18N`, exactly those two commands. Test `ci_workflow::test_i18n_job_builds_both_values`
  (control: the variable renamed, reported). Not run on GitHub: VERIFICATION_DEBT V25.

## N422 — the wasm `simd128` default and the `-Dstrip` and `-Dpie` defaults

- **id**: N422. Parity tail B-2 (N377 items 6 and 8). **severity**: low. **kind**: build-option parity.
  **state**: CLOSED (ported). `simd128` and `-Dstrip`: B-2, commit `40320c7e`. `-Dpie`, and the release
  test runs left unstripped: B-2b (review B-2 F1, F2), commits `7e7fe910`, `c0511017`.
- **simd128**: `Config.zig:113-130` adds the `simd128` CPU feature to every wasm target when no CPU was
  requested (opt-out `-Dcpu=generic`); the reference CI requires the artifact to need it ("Verify WASM
  simd128"). Port: a tracked `.cargo/config.toml` gives `cfg(target_arch = "wasm32")` the rustflags `-C
  target-feature=+simd128`; any `RUSTFLAGS` of the caller replaces them (the `-Dcpu` analog), and
  `cargo rustc ... --` flags add to them. `xtask dist` used to write the vendor source replacement over
  `.cargo/config.toml`; it now appends it to the tracked file, and `distcheck` checks the tracked text
  survived. G-XTARGET now requires `+simd128` in both wasm artifacts' `target_features` section
  (`wasm_target_features.py`, whose `--self-test` the gate runs first; no wasm-tools needed). Measured
  (`rustty-vt-c`, `wasm32-unknown-unknown`, release): before, the section had no simd128 and
  `llvm-objdump -d` (Homebrew LLVM 21) counted 0 SIMD instructions; after, `+simd128` and 4282, also
  through the `cargo rustc ... -- -C link-arg=--export-table ...` form `build-vt-wasm.sh` uses; with
  `RUSTFLAGS=` (the opt-out) 0 and no `+simd128`. Gate: `check_cross_targets.sh wasm32-unknown-unknown
  wasm32-wasip1` GREEN with both simd128 arms; control (the config's rustflags emptied, restored): RED
  `wasm32-unknown-unknown(simd128)`. Tests: `dist::checkout_config_defaults_wasm_to_simd128`,
  `dist_config_appends_the_vendor_replacement_to_the_tracked_config` (control: the tracked text dropped
  fails it). The wasm test suites were not run (no wasm runner on the Mac).
- **-Dstrip** (`Config.zig:414-422`): default false for Debug/ReleaseSafe, true for
  ReleaseFast/ReleaseSmall, applied to the exe (`GhosttyExe.zig:20`) and the embedding library
  (`GhosttyLib.zig:31,102`) only; the test executable is `.strip = false`. Port: `[profile.release]
  strip = true` (the ReleaseFast analog, as `mode_string` reports it), `release-safe` and `bench` `strip =
  false`, and per-package `strip = "debuginfo"` (Cargo's own release default) for the artifacts the
  option does not reach: `rustty-vt-c`, `rustty-bench`, `rustty-synthetic`, `rustty-wasm`,
  `rustty-web-canvas`, `xtask`. The override is Cargo's `CARGO_PROFILE_RELEASE_STRIP` (the flatpak already
  passes `false`); `nix/package.nix` now passes it from its `strip` flag, as the reference passes
  `-Dstrip` (its `releasesafe` builds the `release` profile, so without it that build would be stripped).
  Test: `xtask/tests/strip_defaults.rs` resolves every profile through `inherits` and Cargo's defaults,
  maps it to its Zig mode by `mode_string_for`'s rule and asserts the strip default, plus the bench
  profile and the exact exception list. Controls, one at a time, restored: `strip = true` removed fails
  (`release` is ReleaseFast ... strip = "debuginfo"); the release-safe/bench lines removed fails both.
  Measured (macOS): `rustty` release → 1 defined symbol (`nm -U`), 6.0 MB; with
  `CARGO_PROFILE_RELEASE_STRIP=false` → 5552, 6.5 MB; `--profile release-safe` → 8236;
  `librustty_vt_c.dylib` release unchanged (198 `_rustty_` exports, 4624 defined);
  `librustty_apprt_c.dylib` release stripped to its 99 defined symbols, 95 of them the `_rustty_` exports.
- **not ported, with reasons**: (1) stripped builds drop unwind tables in the reference
  (`unwind_tables = if (strip) .none else .sync`): the port unwinds panics (only the `xcframework` profile
  aborts), so its tables stay. (2) `omitFramePointer()` off Darwin follows `strip`: rustc's frame-pointer
  choice is its target spec's (NON_PORTS G5 covers Darwin), and a profile cannot set it (it would be
  rustflags). (3) The reference's test executable is always Debug, which the release test lanes are
  not (they exist to test the release profile).
- **release test runs unstripped** (B-2b, review B-2 F1): `cargo test --release` builds test executables
  with the `release` profile, not `bench`, so B-2 left them stripped (measured: a release
  `rustty-identity` test binary had 1 defined symbol), while the reference's test executable is `.strip =
  false`. G-REL (`check_release_lane.sh`, before its build step) and ci.sh's `--release` lz4 run now set
  `CARGO_PROFILE_RELEASE_STRIP=false`. Measured (macOS): `check_release_lane.sh --crate rustty-identity`
  GREEN, and the test binary it ran has 1717 defined symbols (`nm -U`); the same build without the
  override, 1. Test: `strip_defaults::release_test_lanes_run_unstripped` (the export precedes both G-REL
  cargo steps; every uncommented ci.sh `cargo test ... --release` carries it). Controls, one at a time,
  restored: the G-REL export removed fails it ("G-REL exports ..."), the ci.sh override removed fails it
  ("ci.sh release test run is stripped"). The `[profile.bench]` comment now names `cargo bench` only.
- **-Dpie** (B-2b, review B-2 F2; `Config.zig:408-412` `orelse system_package`, `GhosttyExe.zig:29-30`
  `if (cfg.pie) exe.pie = true`): the environment variable `RUSTTY_PIE` (`true` / `false`; unset or
  empty = not passed, like `RUSTTY_I18N`). The port has no `--system`, so not passed is the non-system
  default, false: on Linux and FreeBSD `crates/rustty/build.rs` links the `rustty` bin with `-no-pie`
  (rustc passes it after its own `-pie`; the driver takes the last). `true` keeps rustc's PIE; any other
  value fails the build script. Mach-O and PE are untouched. The reference's system packages (`nix`
  `--system`, both flatpak manifests `--system $PWD/vendor/p`) set `RUSTTY_PIE=true`; the snap (no
  `--system`) sets nothing. PACKAGING.md documents it. Tests: `build_checks::pie_option_resolves_like_the_reference`
  (per target, all three values, refused values); `xtask/tests/pie_packaging.rs` (nix and both flatpak
  manifests set it, the snap does not). Controls, one at a time, restored: not-passed resolved to true
  fails the first (`linux`: `Ok(false)` vs `Ok(true)`); the debug flatpak's line removed fails the second.
  Linux VM (aarch64 Fedora, pinned 1.93.1, `cargo build -p rustty`, default features gtk/x11/wayland):
  default → `readelf -h` Type `EXEC`, `FLAGS_1: NOW`, `rustty +version` rc 0; `RUSTTY_PIE=true` →
  `DYN (Position-Independent Executable)`, `FLAGS_1: NOW PIE`, `+version` rc 0; `RUSTTY_PIE=false` →
  `EXEC`; `RUSTTY_PIE=yes` → build error (rc 101, "RUSTTY_PIE must be `true` or `false`, not \"yes\"");
  control (the `-no-pie` line dropped from build.rs, restored) → default build `DYN`. Only the debug
  profile was built on the VM; the release relink with `-no-pie` is the reviewer's measurement (EXEC,
  `+version` rc 0). FreeBSD: UNVERIFIED (no lane).
- **CI** (N376 #6 and #9): G-XTARGET already runs in `ci.sh`, so the simd128 arm runs wherever it does;
  a `build-nix`-style strip check (`nm` of a release `rustty`: no symbols; debug: symbols) is not wired.
  **Wired by parity tail CI-1 (2026-09-28):** `ci.yml` job `build-nix` (x86_64 and arm64) runs the
  reference's checks on the Nix packages: `nm result/bin/.rustty-wrapped` of `.#rustty-releasefast`
  must say `no symbols` (nix/package.nix passes `CARGO_PROFILE_RELEASE_STRIP` from its `strip` flag),
  and that of `.#rustty-debug` must list `main_rustty`, after the `+version` greps for `.ReleaseFast`
  and `.Debug`. The simd128 requirement also gets the reference's own check, `wasm-tools validate
  --features=-simd` must fail, on both wasm tiers in the `build-libghostty-vt-wasm` job and in
  release-tip.yml. Tests: `ci_workflow::build_nix_job_checks_strip_and_mode` (control: the `no
  symbols` grep replaced by `true`, reported; the package and passthru-test names are checked against
  `flake.nix` and `nix/librustty-vt.nix`) and `wasm_job_builds_and_checks_both_tiers`. Not run on
  GitHub: VERIFICATION_DEBT V25 (the Nix side also V24).

## N423 — `zig build run` had no counterpart that sets the resources directory

- **id**: N423. Parity tail B-2 (N377 item 1). **severity**: low. **kind**: build-step parity. **state**:
  CLOSED (ported; the GTK arm is UNVERIFIED on Linux). Commit `dbb360eb`.
- **reference**: `build.zig:282-326`: with an app runtime, run the exe with the build's arguments and
  `GHOSTTY_RESOURCES_DIR=<install prefix>/share/ghostty`; with `.none` on Darwin, the native-only
  xcframework + macOS app, opened (`GhosttyXcodebuild.zig:126-163`); otherwise nothing.
- **fix**: `xtask run [--release | --profile <p>] [--prefix <dir>] [-- <args>]` (`xtask/src/run.rs`):
  Linux/FreeBSD builds `rustty` and runs it with `RUSTTY_RESOURCES_DIR=<prefix>/share/rustty` (prefix
  default `<root>/zig-out`, where `xtask resources` installs; `GHOSTTY_*`/`ghostty` under
  `identity-oracle`), returning its exit status; macOS runs the already-ported app arm
  `macos/run-rustty.sh` (S64-F7) with `XCFRAMEWORK_TARGET=native`; other targets do nothing. HACKING.md's
  run row points at it. Stale record corrected: xtask's "PORT STATUS: scaffold" header.
- **tests**: `run::tests` — the GTK plan (env name and path, exe path per profile, prefix, args,
  identity-oracle names), the macOS plan, the empty plan, strict flags, and `exec` handing the variable
  and arguments to a child and returning its status. Controls, one at a time, restored: the resources dir
  built from the root instead of the prefix fails the GTK plan test; `exec` dropping the environment fails
  the exec test. Not run end to end: the macOS arm opens the GUI app on the user's desktop (not done on
  this host), and the GTK arm needs Linux.

## N424 — dist tarballs named after the bare manifest version; `-Dlib-version-string` missing

- **id**: N424. Parity tail B-2 (N377 items 3 and 5). **severity**: low. **kind**: build-option parity.
  **state**: CLOSED (ported; the nix change is UNVERIFIED, no nix on the Mac). Commits `9633efae`,
  `7cf6de99`.
- **dist naming**: `GhosttyDist.zig:22,52-107` names the tarball, its prefix and `VERSION` (no trailing
  newline) after the full `cfg.version`, for lib-vt too. `xtask dist`/`dist-lib-vt`/`distcheck*` now use
  `rustty_version::VERSION_STRING` (`X.Y.Z` on a `vX.Y.Z` tag, `X.Y.Z-<branch>+<hash>` otherwise,
  `RUSTTY_VERSION_STRING` when set) and write `VERSION` without a newline. The release workflows' `cp
  dist/rustty-${RUSTTY_VERSION}.tar.gz` relies on the tag arm, as the reference's does. The flatpak
  manifests read a fixed `dist/rustty-source.tar.gz`, which `flatpak.yml` (and the manifests' header
  recipe) copies from xtask's output; `snap.yml` already globbed. G-DIST takes the tarball from xtask's
  `wrote` line and checks name, prefix and `VERSION` against the checkout (tag, untagged `+<short hash>`,
  or explicit arm). Run: GREEN, `rustty-0.1.0-tail-b2x+40320c7e.tar.gz`, `VERSION` bytes
  `0.1.0-tail-b2x+40320c7e`, the lib-vt tarball `librustty-vt-0.1.0-tail-b2x+40320c7e.tar.gz`, and the
  shipped `.cargo/config.toml` = the tracked simd128 table + the vendor replacement. Control
  (`DIST_VERSION` back to `"0.1.0"`, restored): RED "untagged checkout: expected 0.1.0-<branch>+40320c7e,
  got 0.1.0". Unit test `dist::tarballs_are_named_after_the_full_app_version`.
- **-Dlib-version-string** (`Config.zig:343-353`): `RUSTTY_LIB_VERSION_STRING` (a build-time variable)
  replaces the vt library version in `rustty-vt`'s `options` (so the C API build-info version; a malformed
  value is a compile error, the `SemanticVersion::parse` const-eval panic) and the vt-c dylib's
  `-current_version`. CMake's `RUSTTY_LIB_VERSION_STRING` cache option validates it (the semver regex),
  passes it to cargo (native and `rustty_vt_add_target`), and versions the install names/soname
  (MAJOR.MINOR.PATCH) and the `.pc` `Version:` (the full string), as `GhosttyLibVt.zig:396-436,532` do
  from `lib_version`; `nix/librustty-vt.nix` passes `finalAttrs.version` as the reference's nix passes
  `-Dlib-version-string`. Tests: `rustty-vt` `options::tests::lib_version_string_option_overrides_the_identity_version`,
  run plain and with `RUSTTY_LIB_VERSION_STRING=1.2.3-rc.1+abc` (both pass; `=1.2` fails the build with
  "invalid semantic version"); control (the option ignored) fails it. `xtask/tests/cmake_lib_vt.rs`
  `lib_version_string_versions_the_library` (configure, build, install with `2.3.4-rc.1+b5`: both `.pc`
  `Version:`s, `librustty-vt.2.3.4.dylib`, the `librustty-vt.2.dylib` link, `current version 2.3.4`) and
  `lib_version_string_must_be_a_semantic_version`; the two cmake-build tests now serialize (they share
  `target/`). Control (the `.pc.in` back on `@PROJECT_VERSION@`) fails the first.
- **CI**: G-DIST runs in `ci.sh` already; the cmake tests run in the xtask test step with
  `RUSTTY_REQUIRE_CMAKE=1`.

## N425 — librustty-vt did not compile for Android: the port read Zig's `.linux` as Rust's `target_os = "linux"`

- **id**: N425. Parity tail A-1. **severity**: medium (the reference's `build-libghostty-vt-android` job
  builds lib-vt for three Android targets; the port could build none). **kind**: port defect (R1), fixed.
  **state**: CLOSED (fixed).
- **finding**: Zig's Android targets are os `.linux` with abi `.android`/`.androideabi`, so every
  `builtin.os.tag == .linux` arm of the reference applies to Android. Rust's Android targets are
  `target_os = "android"`, which `cfg(target_os = "linux")` does not match. With NDK r29 on the Mac,
  `cargo build -p rustty-vt-c --release --target aarch64-linux-android` stopped in rustty-os at four
  `compile_error!` fallbacks (`desktop.rs:114`, `homedir.rs:74`/`:388`, `open.rs:136`); once past those, the
  `mremap` fast path (`mach.rs`, `MREMAP != void` in the reference, and bionic has `mremap`) had no `libc`
  binding for Android, and `armv7-linux-androideabi` failed on `systemd.rs`'s `tv_sec * US_PER_S` (`i32` on
  32-bit Android against the reference's `i64` `toMicroseconds()`). The same misreading also changed behaviour
  that compiled: `rustty-vt`'s `mem.rs` (`canReclaim`/`decommit`, `mem.zig:53,103`) would not have reclaimed
  with `MADV_DONTNEED` on Android, and the type manifest would have said `"os":"unknown"`.
- **fix**: every code-level `target_os = "linux"` in rustty-os, rustty-vt, rustty-vt-c and rustty-build-config
  also admits `target_os = "android"` (an `any(` element gets the sibling, anything else is wrapped in
  `any(target_os = "linux", target_os = "android")`), rustty-os's build script compiles `pty.c` for Android
  (bionic defines `__linux__`), `mach.rs` declares bionic's `mremap` (`MREMAP_MAYMOVE` = 1), and `systemd.rs`
  widens `tv_sec`/`tv_nsec` with `i64::from`. Not changed: rustty-vt's port-only POSIX shared-memory tests in
  `stream_terminal.rs` (the reference has no POSIX shared memory on Android, `graphics_image.zig:202-204`) and
  `graphics_image.rs`, which already named Android.
- **verification**: NDK r29 (29.0.14206865), API 29 (Zig 0.16's default Android API level,
  `std/Target.zig:484`): `cargo build -p rustty-vt-c --release` for `aarch64-linux-android`,
  `x86_64-linux-android` and `armv7-linux-androideabi` all link. The before state is the compile failure
  above. On an Android 15 arm64 emulator with a 16 KiB page-size kernel (N428), the rustty-vt-c unit tests ran
  553/554 (debug), rustty-vt 3180/3199 and rustty-os 105/107; every failure reads the host source tree
  (`CARGO_MANIFEST_DIR` fixtures and headers, `target/` scratch), runs `/usr/bin/true` (Android has
  `/system/bin/true`) or reads `/proc/sys/kernel/osrelease`, which SELinux denies to the adb shell (the
  reference's `getKernelInfo` reads the same file). Host: `cargo test -p rustty-vt`, `-p rustty-os -p
  rustty-vt-c -p rustty-build-config` 3/3 each; clippy `-D warnings` clean for the workspace and for these
  crates at `--target aarch64-unknown-linux-gnu` (`RUSTTY_SYS_CHECK_ONLY=1`).
- **not done**: clippy `--all-targets` for the Android targets stops at
  `missing_const_for_thread_local` on `rustty-vt/src/modes.rs:79`, whose initializer is already `const { .. }`
  (a clippy false positive on Android's `thread_local!` expansion; no CI lane runs clippy for Android).

## N426 — the Android library could not load: rustty-os read errno through `__errno_location`, which bionic lacks

- **id**: N426. Parity tail A-1. **severity**: high for Android (every program linking the library failed at
  load), none elsewhere. **kind**: port defect (R1), fixed. **state**: CLOSED (fixed).
- **finding**: `rustty-os/src/stderr.rs` (the `os/stderr.zig` port behind `logStderr`) declared
  `__errno_location` for Linux and Android, with a PORT NOTE saying bionic has it. Bionic's errno accessor is
  `__errno`. A shared library may link with undefined symbols, so the NDK build succeeded; on the emulator every
  c-vt example failed with `CANNOT LINK EXECUTABLE "./c-vt": cannot locate symbol "__errno_location"
  referenced by ".../librustty_vt_c.so"`. Checking every undefined dynamic symbol of the library against the
  NDK's API-29 bionic stubs (libc, libm, libdl) found this one and no other.
- **fix**: one `errno_location` declaration per libc (`__errno_location` glibc/musl, `__errno` bionic,
  `__error` Apple/BSD) under `#[link_name]`; the PORT NOTE carries a CORRECTION.
- **verification**: after the fix the three Android libraries have 0 unresolved symbols against the API-29
  stubs (55, 53 and 47 undefined, all defined by bionic), and all 23 single-file c-vt examples run on the
  emulator (exit 0). Before: 1 unresolved (`__errno_location`), 23/23 fail to load.

## N427 — the type manifest's `abi` object did not name Android

- **id**: N427. Parity tail A-1. **severity**: low. **kind**: port defect (R1), fixed. **state**: CLOSED (fixed).
- **finding**: `rustty_type_json`'s `abi` is `@tagName(builtin.target.cpu.arch/os.tag/abi)` in the reference.
  On the emulator the reference's `libghostty-vt.so` (built at the pin with Zig 0.16 and NDK r29, `zig build
  -Demit-lib-vt -Dtarget=aarch64-linux-android`) reports `"target":"aarch64","os":"linux",
  "environment":"android"`; the port reported `"environment":"none"`, and would have reported `"target":"unknown"`
  for 32-bit arm. The rest of the 43 KB manifest is identical after the product rename.
- **fix**: `ABI_TARGET` maps `target_arch = "arm"` to `arm`; `ABI_ENVIRONMENT` maps Android to `android`, and
  32-bit arm Android to `androideabi` (Zig's `arm-linux-androideabi`, whose baseline cpu is v7a, is Rust's
  `armv7-linux-androideabi`).
- **test**: `types::tests::manifest_abi_names_the_zig_target_tags` (host and Android). On the emulator it
  passes; negative control (the two Android arms removed, run alone on the emulator): fails with
  `left: ("aarch64", "linux", "android")`, `right: ("aarch64", "linux", "none")`.

## N428 — the Android lane: NDK builds on the Mac, an Android 15 16 KiB-page emulator, and a differential run against the reference

- **id**: N428. Parity tail A-1. **severity**: low. **kind**: verification record + gate. **state**: CLOSED.
- **toolchain** (user-level, no sudo): Homebrew's `android-commandlinetools` cask (already installed) with
  `--sdk_root=~/Library/Android/sdk` (the reference's `android-ndk` default on macOS,
  `pkg/android-ndk/build.zig`); NDK r29 29.0.14206865 (the version `test.yml:901` pins), platform-tools 37.0.1,
  emulator 37.1.11, and `system-images;android-35;google_apis_ps16k;arm64-v8a` (Android 15, API 35, kernel
  6.6.30, `getconf PAGE_SIZE` = 16384). rustup std for the three targets. The procedure is in the tail-wave
  brief (section "Android lane").
- **differential**: the reference's own 23 single-file `example/c-vt*/src/main.c` (renamed
  ghostty->rustty per token casing), linked against the port's aarch64 `librustty_vt_c.so`, print byte-for-byte
  what the same sources print against the reference's `libghostty-vt.so`, on the same emulator; the one
  difference is `c-vt-paste`'s random OSC 5522 password, which differs between any two runs.
- **16 KiB pages (S64-F9)**: every PT_LOAD of all three Android libraries is aligned 0x4000, as is the
  reference's. G-XTARGET (`check_cross_targets.sh`) now builds all three reference targets, probes the
  compiler cc-rs will use (`CXX_<triple>`), and fails an Android leg whose linked library has a PT_LOAD
  aligned below 16 KiB.
- **example drift, not a library gap**: the port's own `example/c-vt-formatter` still formats with
  `rustty_formatter_format_alloc`, while the reference's (924c8a90d) streams through a writer, and the port's
  `example/c-vt-effects` prints desktop-notification and progress-report sections the reference's does not.
  The library side of both matches (the renamed reference sources above run identically).

## N429 — CI has no Android leg (N376 gap 5): the job it needs

- **id**: N429. Parity tail A-1. **severity**: low. **kind**: CI parity (N376 gap 5). **state**: CLOSED by
  parity tail CI-1 (2026-09-28): `ci.yml` job `build-libghostty-vt-android` (below); not run on GitHub,
  VERIFICATION_DEBT V25. Previously: OPEN (ci.yml is batch C-1's).
- **reference**: `build-libghostty-vt-android` (`test.yml:890-936`): matrix `aarch64-linux-android`,
  `x86_64-linux-android`, `arm-linux-androideabi`; `nttld/setup-ndk` r29 (`add-to-path: false`,
  `link-to-sdk: false`, `local-cache: true`); `zig build -Demit-lib-vt -Dtarget=<t>` with `ANDROID_NDK_HOME`.
  Build only; nothing runs on a device.
- **needed**: an `ubuntu-latest` job (or matrix) that runs the same `setup-ndk` step, `rustup target add
  aarch64-linux-android x86_64-linux-android armv7-linux-androideabi`, exports, from
  `${ndk}/toolchains/llvm/prebuilt/linux-x86_64/bin`, `CC_<t>`/`CXX_<t>` = `<clang-triple>29-clang(++)`,
  `AR_<t>` = `llvm-ar` and `CARGO_TARGET_<T>_LINKER` = the same clang, for each target (clang triples
  `aarch64-linux-android`, `x86_64-linux-android`, `armv7a-linux-androideabi`), and runs `bash
  .port/tools/gates/check_cross_targets.sh aarch64-linux-android x86_64-linux-android
  armv7-linux-androideabi`. That builds all three and fails on a PT_LOAD below 16 KiB. Without the NDK the
  Android legs report `skipped`, so the existing G-XTARGET run in `ci.sh` does not cover them. Because the
  job names the three targets, a named target that is skipped (no `CXX_<t>`, or a toolchain-matched link
  error) makes the run BLOCKED (exit 3), never GREEN on the legs that did build (review A-1 F1, task A-1b);
  the job must therefore treat exit 3 as a failure, not as a warning.
- **fix (CI-1)**: `build-libghostty-vt-android`, the reference's three-row matrix (`needs: ci`, in
  `required.needs`) on `ubuntu-latest`: `nttld/setup-ndk@ed92fe6c... # v1.6.0` with `ndk-version: r29`,
  `add-to-path: false`, `link-to-sdk: false`, `local-cache: true` and `ANDROID_NDK_VERSION: r29`, as the
  reference; `rustup target add` of the row's target; then, from
  `${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin`, `CC_<t>`/`CXX_<t>` =
  `<clang-triple>29-clang(++)`, `AR_<t>` = `llvm-ar`, `CARGO_TARGET_<T>_LINKER` = the clang (clang
  triple `armv7a-linux-androideabi` for `armv7-linux-androideabi`), and `check_cross_targets.sh
  "$TARGET"`, whose exit status is the step's: 3 (BLOCKED) fails the row with an `::error::` saying so,
  and nothing (`continue-on-error`, `|| true`) lets it pass. One target per row, like the reference's
  matrix; G-XTARGET's named-target rule makes a skipped row BLOCKED. Test
  `ci_workflow::android_job_follows_n429` (in-test controls: exit 3 accepted, `continue-on-error`,
  NDK r27, the armv7 clang triple; file-level control: r29 -> r27, RED).

## N430 — WASI `Async::notify` from a callback formed `&Loop` and wrote `Loop.wakeup` while `run(&mut self)` was live

- **id**: N430. Parity tail X-2 (review X-1, section 5). **severity**: low (aliasing-model UB, not a reproduced
  failure). **kind**: port soundness (Stacked and Tree Borrows). **state**: CLOSED (fixed by N431, no separate change).
- **sites**: `crates/rustty-xev/src/wasi_watcher.rs:192` `Some(w) => unsafe { (*w.loop_).async_notify(w.c) }`;
  `wasi_poll.rs` `async_notify(&self, c)` stores `self.wakeup`, the loop's own field, which `tick_inner` then reads
  and clears. The loop thread held the protected `&mut Loop` of `run` -> `tick` -> `tick_inner` across every
  callback, so a timer or I/O callback that notifies an `Async` whose waiter holds the stable root (as
  `AsyncTests` and termio do) wrote a byte that protected borrow covers.
- **reachability**: WASI's `Async` exists only without the wasm `atomics` feature (single thread), so the notify
  can only come from a callback on the loop's own thread.
- **evidence**: `.port/phase-2/mirihar-n431` (`bash run-all.sh`, miri nightly 2026-07-22): `shipped_thread_wasi`
  and `shipped_local_wasi` UB under SB ("strongly protected") and TB ("reborrow ... is forbidden"); with the N431
  raw-self tick path, `raw_thread_wasi` and `raw_local_wasi` are CLEAN under both, with `notify` unchanged. The
  review's local fix (the flag moved out of the `Loop` allocation) is therefore not applied.
- **tests**: `wasi_poll.rs` `wasi_callback_reenters_through_stored_loop_pointer` (PORT-ADDED with N431) compiles
  for `wasm32-wasip1` (clippy `--all-targets -D warnings` clean); like every WASI test it runs only under a wasm
  runtime (VERIFICATION_DEBT V8). UNVERIFIED by execution.

## N431 — every xev backend held a protected `&mut Loop` across callbacks that re-enter the loop

- **id**: N431. Parity tail X-2 (review X-1 O1). **severity**: medium (aliasing-model UB on termio's normal write
  path; not a reproduced failure). **kind**: port soundness (Stacked and Tree Borrows). **state**: CLOSED (fixed).
- **finding**: libxev's `run(self: *Loop)`/`tick`/`tick_inner` take an unprotected Zig pointer. The port took
  `&mut self` in every backend (`kqueue.rs`, `epoll.rs`, `io_uring.rs`, `iocp.rs`, `wasi_poll.rs`, plus the
  `dynamic.rs` dispatch), so a function-entry, protected `&mut Loop` was live across every callback. Callbacks
  re-enter the loop (a) through a pointer the caller stored: termio's `thread.rs` ran `(*self_).loop_.run(..)`,
  and the wakeup callback's `exec.rs` `td.loop_` (= `&raw mut (*thread).loop_`, `termio_impl.rs:921`) reaches
  `stream.rs:461` `(*loop_).add(..)`; the stop callback calls `(*self_).loop_.stop()`; the renderer thread
  (`rustty-core` `renderer_seam.rs`) and the cf release thread (`rustty-os` `cf_release_thread.rs`) have the same
  shape; and (b) through the callback's `l` after `tick_inner` wrote a loop field through `self` (e.g.
  `self.active -= 1` just before `cb(..)`). A third window: epoll/wasi `start` and iocp `start_completion`
  invoke callbacks synchronously from the tick path under their own `&mut self`.
- **fix (R1: behaviour unchanged)**: the tick path is raw-self, as Zig's `self: *Loop`. In every backend,
  `pub unsafe fn run_raw(this: *mut Loop, mode)` and `tick_raw` hold the reference bodies; `tick_inner` (and
  io_uring's `tick_`) take `this: *mut Loop`, and so do the callback-invoking helpers (`start`/`stop_completion`
  on epoll and wasi_poll, `start_completion` and a private `submit_raw` on iocp). They use narrow
  `(*this).field` accesses, and call `&mut self` helpers only for short reborrows that invoke no callback.
  `run(&mut self)`/`tick(&mut self)`/iocp `submit(&mut self)` stay as thin entries that convert to raw at once.
  Callers whose callbacks re-enter through a stored pointer enter through `Loop::run_raw` with that pointer:
  termio `thread.rs` (both runs), `renderer_seam.rs` (the thread and its compression test harness),
  `cf_release_thread.rs` (both runs), the three `exec.rs` KATs (with `loop_ptr`; `before_first_tick` now gets
  `&mut *loop_ptr`) and `tests/live_shell.rs`. PORT NOTE on each backend's `run`; LIFETIMES rows
  `Termio.zig ThreadData.loop` and `async.zig AsyncLoopState.waiter.loop` annotated.
- **residual (documented, not reached by the app)**: a public `&mut self` method still carries its own protector.
  A caller that invokes one through a stored pointer, and whose synchronously-fired callback re-enters through
  the same pointer, is outside this fix. Of the public methods, only iocp `submit` fires callbacks synchronously
  (the job-object arm). `run(&mut self)` is sound when callbacks reach the loop only through `l`, which is how
  every rustty-xev unit test is written.
- **evidence**:
  - `.port/phase-2/mirihar-n431` (isolated, non-FFI, mirrors the tick bodies). `bash run-all.sh`, miri nightly
    2026-07-22, 20 candidates x SB/TB. Shipped shape: `shipped_{thread,local}_root` UB/UB,
    `shipped_{thread,local}_l` UB/CLEAN, `shipped_{thread,local}_wasi` UB/UB, `shipped_thread_sync` UB/UB.
    Thin wrapper entered from the stored root: `wrapper_*_root` UB/UB; with only `l`, `wrapper_*_l` CLEAN/CLEAN.
    Raw tick with `start` kept `&mut self`: `rawmutstart_thread_sync` UB/UB. The applied shape,
    `raw_{thread,local}_{root,l,wasi,sync}`: all 8 CLEAN/CLEAN. `cargo +nightly miri test` there passes under
    both models.
  - Real code under Miri: the kqueue backend cannot run under Miri ("can't call foreign function `kqueue`";
    its loop/async/timer tests are MIRI EXCEPTION C3-C5). The epoll backend can, from the Mac, via
    `cargo +nightly miri test -p rustty-xev --target aarch64-unknown-linux-gnu` (Miri shims epoll and eventfd).
    PORT-ADDED `epoll_callback_reenters_through_stored_loop_pointer`: CLEAN under SB and TB. Entered through
    `(*root).run(..)`: UB under both ("strongly protected" / "reborrow ... is forbidden"). The same test on the
    pre-N431 code (entered through `run`): UB under both. The other epoll loop/timer/async tests give the same
    verdict before and after. Two unrelated SB results predate this change: `epoll_timer_reset` and
    `epoll_timer_cancellation` are UB under SB and clean under TB, in the timer heap. `epoll_timerfd` is
    unsupported (`timerfd_create`). The rustty-xev `--lib` Miri suite (21 non-FFI tests) passes under SB and TB.
- **tests**: PORT-ADDED `<backend>_callback_reenters_through_stored_loop_pointer` in `kqueue.rs` (MIRI EXCEPTION C3),
  `epoll.rs`, `io_uring.rs`, `iocp.rs` and `wasi_poll.rs`. A timer callback arms a second timer through the stored
  pointer, the second stops the loop through it, and `run_raw(.., UntilDone)` returns only if both ran.
  Mac: `cargo test -p rustty-xev -p rustty-termio` 3/3, `cargo test -p rustty-core` 1/1. Linux (rvm, epoll and
  io_uring): clippy clean, `cargo test -p rustty-xev -p rustty-termio` 3/3 (xev 108 passed each). Windows (rwin,
  `--target x86_64-pc-windows-msvc`): clippy `-p rustty-xev -p rustty-termio --all-targets -D warnings` clean,
  `cargo test -p rustty-xev` 3/3 (45 passed each).

## N432 — `Async.wait` rewrites the completion before taking the lock, racing a concurrent `notify` (IOCP)

- **id**: N432. Parity tail X-2 (review X-1 O2). **severity**: low. **kind**: latent reference shape
  (record only). **state**: OPEN (record only; not a UD, unreachable in Ghostty).
- **shape**: `AsyncIOCP.wait` writes `c.* = .{ .op = .{ .async_wait = .{} }, ... }` and calls `loop.add(c)`
  before `self.guard.lockUncancelable(io)` (reference libxev 86vtcwIR `src/watcher/async.zig:568`, `:587`, `:590`);
  the port is the same (`crates/rustty-xev/src/iocp_watcher.rs` `wait`: `*c = Completion::new_op(..)`, then
  `(*loop_).add(c)`, then `self.guard.lock()`). `notify` on another thread, holding `guard` with the PREVIOUS
  waiter (the same `c`), reads `c.op` and stores its `wakeup` atomic. If a caller disarms an async wait and later
  calls `wait` again with the same completion while another thread notifies, the rewrite races that access (a
  data race in both languages) and the notification can be lost.
- **reachability**: none in Ghostty. Every `Async.wait` call (termio `Thread.zig:273-274`, renderer
  `Thread.zig:238-240`, `os/cf_release_thread.zig:133-134`) runs once at thread start. Their callbacks return
  `.rearm`, or `.disarm` and stop the loop, and never call `wait` again. So it is not a UD (not reachable), and
  the port stays faithful.

## N434 — the xev timer heaps linked a Timer through a `&mut (*c).op` borrow that the next `&mut (*c).op` invalidated

- **id**: N434. Parity tail X-3 (review X-2 F1; X-2 report concern 1). **severity**: medium (aliasing-model UB on
  the macOS renderer thread's timer reset and cancel; no observed miscompile). **kind**: port soundness (Stacked
  Borrows; Tree Borrows once two timers are linked). **state**: CLOSED (fixed).
- **finding**: Zig reaches the timer as the place `c.op.timer` and links that `*Timer` in the heap. The port
  derived the linked pointer from a `&mut (*c).op` reborrow: epoll `start` (an inner `match &mut
  (*completion).op`), kqueue `start`, iocp `start_completion` and wasi_poll `start` (the binding of a whole-op
  `match &mut (*c).op`). While the timer is linked, `timer_reset`'s Active arm and `stop_completion` formed a
  fresh `&mut (*c).op`, a Unique retag from the completion's own tag that pops the heap's pointer under SB;
  `heap::remove` -> `delete_min` -> `get_child` then read the node through the popped root link. Under TB, once
  the heap wrote the node's links through that pointer (a second timer melded under it), the retag's read
  froze it and `delete_min`'s `root.heap = .{}` write is forbidden. Reachable in the app on macOS through kqueue:
  `renderer_seam.rs` `render_h.reset` (`armAnimationTimer`), `cursor_h.reset` and `cursor_h.cancel`. The
  `heap.rs` pairing heap itself is raw end to end (no `&mut` to a node) and needed no change. io_uring has no
  user-space timer heap.
- **fix (R1: same writes, same order)**: one helper per backend, `unsafe fn timer_ptr(c: *mut Completion) ->
  *mut Timer`, takes a shared `&(*c).op` only to find the payload address and returns it with `c`'s own
  provenance (`c.cast::<u8>().with_addr(addr).cast()`). Every heap-path Timer access goes through it: the
  `start` insert and its `.c` write (the arm is now `Operation::Timer(_)`), `timer_reset`'s Adding and Active
  arms, and `stop_completion`'s remove and `reset`/`next` accesses, whose match is now shared (`&(*c).op`;
  kqueue `if let` -> `matches!`, iocp/wasi `if let` in `timer_reset` -> `matches!` guard, so the non-timer no-op
  is kept). PORT NOTE on each helper. LIFETIMES row `heap.zig Intrusive(T) root` annotated.
- **evidence**:
  - `.port/phase-2/mirihar-n434` (isolated, non-FFI; the REAL `heap.rs` included by `#[path]`; the renderer
    Thread shape with three timers). `bash run-all.sh`, miri nightly 2026-07-22, 32 candidates x SB/TB. Shipped
    shape, all four backends: `*_shipped_{reset,cancel}_root` UB/UB (SB "tag does not exist in the borrow
    stack" at `heap.rs:39`; TB "write access .. is forbidden .. Frozen" at `heap.rs:146`);
    `*_shipped_{reset,cancel}_inner` CLEAN/CLEAN (a non-root node's stale link is only compared by address).
    Fixed shape: all 16 `*_fixed_*` CLEAN/CLEAN; `cargo +nightly miri test applied` passes under SB and TB.
  - Real code under Miri (epoll, from the Mac: `cargo +nightly miri test -p rustty-xev --target
    aarch64-unknown-linux-gnu --lib -- --exact epoll::tests::<name>`), 14 loop/timer/async/cancel tests: before
    the fix `epoll_timer_reset` and `epoll_timer_cancellation` are UB under SB (single timer, so CLEAN under TB),
    the other 12 CLEAN/CLEAN; after the fix all 14 CLEAN under SB and TB. The 6 lib tests matching `heap` pass under
    Miri SB and TB. kqueue, iocp and wasi_poll cannot run under Miri (kqueue: "can't call foreign function
    `kqueue`", MIRI EXCEPTION C3); they are proven by the harness and the shared code shape.
- **tests**: Mac `cargo test -p rustty-xev -p rustty-termio` 3/3 (xev 52 passed + 1 ignored, including the
  live `kqueue_timer_reset`/`kqueue_timer_cancellation`), `cargo test -p rustty-core` 1/1. Linux (rvm, `-j 4`):
  `cargo test -p rustty-xev -p rustty-termio` 3/3 (xev 108 passed, termio 92 + 6 ignored). Windows (rwin,
  `--target x86_64-pc-windows-msvc`): `cargo test -p rustty-xev` 3/3 (45 passed, including `iocp_timer_reset`
  and `iocp_timer_cancellation`). WASI: compiles and clippy-clean only; no runtime (VERIFICATION_DEBT V8),
  UNVERIFIED by execution.

## N435 — `ScreenSearch::feed` carried the reference's `.complete => unreachable` after history is prepended

- **id**: N435. Phase 7 start (P7-W0 T4; R1 #1, DECISIONS R1). **severity**: medium (a reachable panic).
  **kind**: reference crash ported faithfully, unrecorded. **state**: CLOSED by P7-W5 T4, by the port of
  `06178eeaa`.
- **reference**: at 20abdb50a, `src/terminal/search/screen.zig:308` `feed` switches on the state with
  `.complete => unreachable` (`:343`), under the comment "If we're complete then the feed call above should always
  return false and we can't reach this." Upstream found that it is reachable and fixed it in the range:
  `06178eeaa` ("terminal/search: resume a complete search when history is prepended") makes the arm
  `.complete => self.state = .history` (`search/screen.zig:354` at 12752b2ac).
- **rustty**: `crates/rustty-vt/src/search/screen.rs:569` `feed`, `State::Complete => unreachable!()` (`:619`),
  a faithful port of the pin.
- **reachability**: a screen search that has already exhausted its PageList (`.complete`) is fed again after
  older history pages are prepended below it (incremental snapshot history restore); the history searcher's
  tracked pin keeps its node identity through the prepend, so the feed finds new pages and reaches the arm.
  By reading (R1); reproduced by the pre-fix `feed_after_complete_discovers_prepended_history_pages` test
  panicking on `unreachable!()` (`crates/rustty-vt/src/search/screen.rs:632`, recorded in the P7-W5 T4 commit).
- **why not a UD**: upstream found and fixed it within the range, so at the new pin it is not an R13 divergence;
  the port adopts `06178eeaa` as is, with its tests.
- **fix** (P7-W5 T4): `crates/rustty-vt/src/search/screen.rs` `feed`'s `State::Complete` arm now sets
  `self.state = State::History` (`06178eeaa`, `search/screen.zig:354` at 12752b2ac), with doc updates on
  `State::needs_feed` and `ScreenSearch::feed`. Ported all 3 upstream tests, names and assertions preserved:
  `feed_discovers_pages_prepended_after_exhaustion` (`crates/rustty-vt/src/search/pagelist.rs`),
  `feed_after_complete_discovers_prepended_history_pages` (`crates/rustty-vt/src/search/screen.rs`), and
  `feed_after_complete_discovers_prepended_snapshot_history` (`crates/rustty-vt/src/search/terminal.rs`).
  Verified: targeted `cargo test -p rustty-vt search` (92 passed) and `-p rustty-vt-c search` (24 passed);
  clippy `-D warnings`; fmt; `cargo build --workspace`; `check_ud_docs.py` PASS. Miri (bounded at 1800s per
  run, foreground): `cargo +nightly miri test -p rustty-vt search::` ran `feed_discovers_pages_prepended_
  after_exhaustion` clean (`ok`) but hit the 1800s bound partway through
  `feed_after_complete_discovers_prepended_history_pages` (exit via SIGALRM, not a failure) before reaching
  `feed_after_complete_discovers_prepended_snapshot_history`; both are heavier PageList-integrity tests of
  the same class already carrying "MIRI EXCEPTION — WS19 T6" budget-outs elsewhere in this file (Page::
  assert_integrity is O(page) under debug_assertions), so this is consistent with that documented cost, not
  a new UB signal. `pagelist::builder` matched 0 tests (no test's fully-qualified path contains that literal
  substring — the allocate_page/`Location::Prepend` path this fix exercises is covered by the Miri-clean
  `feed_discovers_pages_prepended_after_exhaustion` above instead). Not retried per the bounded-run rule;
  flagged as a follow-up for whoever next extends the WS19 T6 Miri-budget table.

## N436 — `ghostty_search_tick` read the freed terminal's PageList after `ghostty_terminal_free`

- **id**: N436. Phase 7 start (P7-W0 T4; R1 #37, R3 §3.5, DECISIONS R1). **severity**: medium (use-after-free
  through the C API). **kind**: reference use-after-free ported faithfully, unrecorded. **state**: CLOSED
  (P7-W2 T1) by the port of `b1c264163`.
- **reference**: at 20abdb50a, `src/terminal/c/search.zig:170-178` `tick` checks only the search handle, then
  runs `s.tick()`. After the terminal is freed, a history search's tick reads `history.start_pin`, a pin tracked in
  the freed PageList. `b1c264163` ("vt: reject search ticks after the terminal is freed") adds
  `_ = wrapper.zigTerminal() orelse return .invalid_value;` (`c/search.zig:176` at 12752b2ac), an assertion in
  "search free after terminal free" (`:949`), the new test "search tick after terminal free with history"
  (`:988`), and the `search.h` doc.
- **rustty**: `crates/rustty-vt-c/src/search.rs:508-525` `tick` has no terminal check, so by reading it carries
  the same use-after-free. The guard ports through the raw-self `zig_terminal` helper (`:110`).
- **fix**: `crates/rustty-vt-c/src/search.rs` `tick` now calls
  `SearchWrapper::zig_terminal(search_)` and returns `Result::InvalidValue` when it is null, before touching
  `search`. `search_free_after_terminal_free` gained the `tick(search, null)` -> `InvalidValue` assertion
  (matching upstream's `:949` insertion), and the new test `search_tick_after_terminal_free_with_history` ports
  upstream's "search tick after terminal free with history" (`:988`) assertion-for-assertion. The
  `include/rustty/vt/search.h` doc for the lifetime section and `rustty_search_tick`'s `@return` were updated
  verbatim from the oracle's `search.h` diff.
- **evidence**: `cargo +nightly miri test -p rustty-vt-c search` — RED before the guard (with the new test
  present, no guard): `error: Undefined Behavior: memory access failed: ... has been freed, so this pointer is
  dangling` in `rustty_vt::search::screen::ScreenSearch::tick_history`, reached through
  `search::tick` -> `TerminalSearch::tick` -> `ScreenSearch::tick`
  (`search_tick_after_terminal_free_with_history` failed; `search_free_after_terminal_free` also failed its
  `InvalidValue` assertion). GREEN after the guard: `test result: ok. 22 passed; 0 failed; 2 ignored` (the 2
  ignored are the pre-existing MIRI EXCEPTION rows, unrelated to this change). Native
  `cargo test -p rustty-vt-c search`: `24 passed; 0 failed`.
- **commit**: `search: reject search ticks after the terminal is freed (b1c264163) (P7-W2 T1)`.
- **why not a UD**: upstream found and fixed it within the range (as N435).

## N437 — the render thread forms whole-`Renderer` references that overlap the main thread's raw sync draw (macOS)

- **id**: N437. Phase 7 start (P7-W0 T4; the P7-W0 T1 review, controller ruling 7). **severity**: medium
  (aliasing-model UB on the macOS renderer every frame; no observed miscompile). **kind**: port soundness (Stacked
  and Tree Borrows); pre-existing, not introduced by `72cf50855`. **state**: CLOSED (fixed, P7-W7 T2).
- **state history**: opened OPEN at P7-W0 T4, owned by P7-W7 T2 (the renderer-lifecycle rework), which
  closed it.
- **finding**: on macOS CoreAnimation calls `draw_frame_raw(sync = true)` on the main thread
  (`crates/rustty-core/src/renderer_seam.rs:492`, `:496`), which reaches the renderer through a raw pointer and
  takes `draw_mutex`. Meanwhile the render thread calls whole-`Renderer` `&mut self` / `&self` methods through
  `(*(*self_).renderer).0`: `update_frame` (`renderer_seam.rs:1823`), `animation_wake` without the lock
  (`:1313`, `armAnimationTimer`), `set_visible` (`:1396`), `set_focus` (`:1424`), `loop_enter` (`:1164`) and
  `set_macos_display_id` (`:1583`). Each forms a reference over the whole renderer, and a function-entry
  `&mut self` is protected for the whole call, so a concurrent main-thread access through the raw pointer to any
  field (the sync draw's `&mut` reborrows included) is UB under Stacked and Tree Borrows, whether or not the two
  touch the same bytes and whatever `draw_mutex` serialises. Zig's `*Renderer` has no such aliasing rule. By
  reading; not yet shown in a Miri harness.
- **follow-ups for P7-W7 T2**:
  - UD352's port-handling text says "after the unlock the render thread touches no renderer bytes". That
    overclaims for `update_frame`: its receiver is a `&mut Renderer` for the whole call, the unlocked resync
    included. Reword UD352 when the rework lands.
  - `DisplayLinkHandle` (`crates/rustty-renderer/src/metal/cvdisplaylink.rs:251-280`) is a safe `pub`,
    `Copy` API over a non-owning `CVDisplayLinkRef` with no lifetime, so safe code can call `start`/`stop` after
    the owning `DisplayLink` is released. Make `DisplayLink::handle` `pub(crate)`, or make the handle's methods
    `unsafe` with a `# Safety` section.
- **fix direction**: raw-self receivers with narrow field access for every render-thread entry that can overlap
  the sync draw (memory `shaper-hook-sb-clean-interleave`), with LIFETIMES rows first and an isolated Miri
  harness under `.port/phase-2/mirihar-<name>/` that is UB before and clean after.
- **fix** (P7-W7 T2): `draw_mutex` moved out of the renderer into its own allocation
  (`rustty_renderer::latest_frame::DrawMutex`, an `Arc<Mutex>` handle); core holds one handle for the render
  thread (`renderer_seam::Thread::draw_mutex`) and one for the main thread (`RendererHandles`, beside the
  renderer). Every cross-thread entry in both `generic` twins is a raw-self `*_raw` entry or goes through
  `Renderer::with_locked`, which locks through the caller's handle BEFORE forming any renderer reference; the
  whole `&mut` exists only under the lock. The entries the reference runs unlocked (`setFocus`, `setVisible`,
  `animationWake`, `hasVsync`, `loopEnter`, `setMacOSDisplayID`) lock too. The latest-frame slot is an
  `Arc<LatestFrame>`, so the GTK main thread's `takeFrame` touches no renderer byte. The macOS display
  callback's context carries the renderer pointer and a `DrawMutex` handle (`metal_impl::DisplayCallbackCtx`).
  LIFETIMES rows added. Both follow-ups done: UD352 reworded; `DisplayLink::handle` is `pub(crate)` and the
  `DisplayLinkHandle` methods are `pub(crate) unsafe fn` with `# Safety` sections.
- **proof**: `.port/phase-2/mirihar-n437/` (the real `Mutex` and `latest_frame` by `#[path]`; 16 seeds, SB and
  TB): the three shipped shapes (`update_frame`, unlocked `animation_wake`, `set_focus` vs the raw sync draw)
  are UB under both; two negatives (lock-first with the mutex inline; external lock with the frame slot inline)
  are UB under both; the fix against the macOS sync draw and against the GTK main thread is clean under both.
- **residual** (not in N437's scope): the Metal completion handler is **N440**; the main thread's
  whole-core-`Surface` references over the embedded renderer are **N441**. Both OPEN. (N441 CLOSED
  by P7-W7 T5b.)
- **ruling 32 follow-up** (P7-W7 T2 fix round 1): `update_frame` keeps the reference's lock scopes;
  the unlocked part uses a narrow view, and the main thread's locked entries are narrow too
  (`display_realized_raw` / `display_unrealized_raw`, the sync draw's `*_raw` helpers). Harness
  candidates `split_*` in `.port/phase-2/mirihar-n437` (narrow main side CLEAN, whole-renderer main
  side UB, SB and TB).

## N438 — the reference's fontconfig archive omits `fcconffile.c`, leaving two public functions undefined

- **id**: N438. P7-W10 T1 (R4 §A fact 2; DECISIONS B14). **severity**: none observable. **kind**: latent
  reference shape (record only). **state**: OPEN (record only; not a UD, unreachable).
- **shape**: the reference's `srcs` list (`pkg/fontconfig/build.zig:343-373`) has 29 translation units; upstream's
  meson build also compiles `src/fcconffile.c` (`src/meson.build:25`). That file defines the public
  `FcConfigFileGenerate` and `FcConfigFileGenerateGenericAlias` (declared in `fontconfig.h:1224` onward). No
  other TU calls them, so the static archive links, but a caller of either would get an undefined symbol.
- **reachability**: none. Neither Ghostty nor the port calls them (the `fc` module binds neither), and nothing
  inside fontconfig does.
- **port**: faithful. `fontconfig-sys` vendors `src/fcconffile.c` (the tarball's `src/*.c`) but leaves it out of
  `SOURCES`, with a comment citing B14 and this note. Evidence: `nm` of the macOS and Linux (rvm) archives lists
  no `FcConfigFileGenerate*` symbol.
- **not here**: R4 §A fact 1 (the stale `FC_VERSION_*` defines), first recorded under this id as latent, is
  observable (it stamps and compares cache files) and is fixed as UD360.

## N440 — the Metal completion handler reaches the renderer without the draw mutex while the render thread holds a whole-renderer `&mut`

- **id**: N440. P7-W7 T2 fix round 1 (review IMPORTANT 3, controller ruling 33). **severity**: medium
  (aliasing-model UB on macOS on frames that complete while the render thread is inside a locked
  entry; no observed miscompile). **kind**: port soundness (Stacked and Tree Borrows). **state**: CLOSED
  by P7-W7 T5a (both halves; see "closure" below).
- **finding**: Metal command buffers complete on a Metal-owned thread, which runs
  `metal_buffer_completed` (`crates/rustty-renderer/src/generic.rs`, the inlined
  `metal/Frame.zig:79-91` `bufferCompleted`). It recovers the renderer as a raw pointer and calls
  `(*renderer).api.present(..)` (a `&mut` reborrow of `api`) and `(*renderer).frame_completed(health)`
  (a `&self` reborrow of the whole renderer; it reads `health`, `surface_mailbox` and `swap_chain`),
  without taking `draw_mutex`. Meanwhile the render thread may hold a whole-renderer `&mut` under the
  lock (`Renderer::with_locked`, e.g. the locked tail of `update_frame_impl`, `set_font_grid_locked`,
  the search-match writes), whose retag and accesses cover the same bytes: a data race with a retag and
  a foreign access to protected bytes. The reference's `*Renderer` has no such rule; its fields are
  either atomics (`health`) or only read. By reading; not yet shown in a harness.
- **fix direction**: give the completion handler what it needs outside the renderer's bytes: move
  `health` and the swap chain's frame semaphore behind an `Arc` (as `LatestFrame`), hand the handler a
  clone of the surface mailbox and a raw `*const` to `api`'s presentation state (or an `Arc`ed
  presenter), so it forms no reference to the renderer; prove it on a `.port/phase-2/mirihar-*`
  harness (UB before, clean after, SB and TB). UD362's by-reading Metal note (the `.renderer_health`
  `.forever` push before `releaseFrame`) belongs in the same task.
- **update (fix round 2)**: on the SYNC path the trampoline runs on the main thread under the lock; its
  whole-renderer `&self` raced the ruling-32 render side, so `frame_completed` is now
  `frame_completed_raw` (place expressions on `health`, `surface_mailbox`, `swap_chain` only;
  harness `split_vs_sync_draw_trampoline_whole` UB, `split_vs_sync_draw_narrow` CLEAN). The async
  (completion-thread, unlocked) half of this finding stays OPEN.
- **Metal deadlock variants to fix in the same task** (UD362 extension, ruling 36): (a) the completion
  thread's `.renderer_health` `.forever` push precedes `release_frame`, while the main thread's sync
  draw holds `draw_mutex` in `next_frame` → `frame_sema.wait()` with all three frames in flight and a
  full mailbox; (b) on the sync path the main thread itself pushes `.renderer_health` `.forever` into
  the mailbox only it drains. Fix direction: `release_frame` before the push, and never block the
  drainer on its own mailbox (defer the push past the draw, as the OpenGL twin does, or push
  `.instant` from the main thread).
- **closure (P7-W7 T5a)**: the async half is fixed. `crate::metal::frame::CompletionContext` no longer
  carries the renderer or target pointers; it carries owned clones, each in its own allocation, taken
  under the draw mutex when the frame begins (`draw_frame_locked`): `health: Arc<AtomicHealth>` (the
  renderer's `health` field is now that `Arc`), `frame_sema: Arc<Semaphore>` (the swap chain's
  semaphore is now that `Arc`), a `SurfaceMailbox` clone, `layer: Arc<LayerCell>` (Metal's `layer`,
  the presenter state, now in its own cell; `LayerCell` is `Send + Sync` by a documented SAFETY note)
  and a retain of the target's IOSurface (all `present` reads of the `Target`). The handler
  (`metal_buffer_completed` → `metal_impl::present_layer` + `frame_completed_on`) forms no reference
  to any renderer byte, on either path. `frame_completed_raw` is gone; `Renderer::frame_completed`
  is a test-only method over the renderer's own cells. Proof: `.port/phase-2/mirihar-n440`
  (`shipped_async_handler` UB under SB and TB, run before the change; `fixed_async_handler` and
  `fixed_async_and_sync_draw` CLEAN under SB and TB, 16 seeds each). LIFETIMES rows: `Frame.zig`
  `CompletionBlock.Context` `renderer`/`target`, `generic.zig` `Renderer.health`,
  `SwapChain.frame_sema`, `Metal.zig` `Metal.layer`. The two Metal deadlock variants are fixed in the
  same task (UD362). Fix round 1 (ruling 39): the reference's order (store, push, release) is kept with
  an `.instant` push plus withdrawal and a `HealthReportRetry` (redraw flag + `draw_now` wakeup, also in
  the context), so no completion can outlive `SwapChain::deinit`; `LayerCell` is narrowed (private
  field, only `present` reachable, `Metal::layer()` for the rest); the harness tears the renderer down
  before joining the completion thread and models the new order (still UB / CLEAN / CLEAN). Fix round 2
  (ruling 41): the retry state is `HealthReport` (`reported` under a mutex, `pending`, a paced `draw_now`
  wakeup that skips while the display link runs), shared by the renderer (`health_report`) and the
  context; the harness models `reported` (still UB / CLEAN / CLEAN). Fix round 3: the wake is sent only when a
  report becomes owed, and core's `drawNowCallback` arms the animation timer while one is owed
  (`Renderer::health_report_owed_raw`), so no refused report is stranded.

## N441 — main-thread references to the whole core `Surface` cover the renderer the render thread writes under the draw mutex

- **id**: N441. P7-W7 T2 fix round 1 (review IMPORTANT 3, controller ruling 33). **severity**: medium
  (aliasing-model UB on every platform whenever a main-thread surface callback overlaps a render-thread
  locked write; no observed miscompile). **kind**: port soundness (Stacked and Tree Borrows).
  **state**: CLOSED (fixed, P7-W7 T5b).
- **state history**: opened OPEN at P7-W7 T2 fix round 1 as a follow-up task; P7-W7 T5b closed it.
- **finding**: the core `Surface` embeds the renderer (`renderer: renderer_seam::Renderer`, whose `.0`
  is the `rustty_renderer::Renderer`). The GTK main thread calls core methods through a whole
  `&mut Surface`: `unsafe { &mut *core }.contentScaleCallback(..)` and `.sizeCallback(..)`
  (`crates/rustty-apprt/src/gtk/class/surface.rs`, `render_surface_resize`), and the other
  `&mut self` / `&self` methods of `crates/rustty-core/src/surface.rs` the apprt calls (key, mouse,
  focus, clipboard callbacks). Each retag of the whole `Surface` covers the renderer's bytes, which the
  render thread writes under `draw_mutex` through its own raw root (N437's fix): a data race with a
  retag. N437 made every renderer entry lock-first and narrow; it did not change how the apprt reaches
  the core surface.
- **fix direction**: move the renderer out of the `Surface` allocation (a `Box`/raw pointer the render
  thread and the surface share, as the terminal already is), or make the main-thread core entries
  raw-self with narrow access that never spans `renderer`. Harness first (`.port/phase-2/mirihar-*`),
  UB before and clean after under SB and TB.
- **fix** (P7-W7 T5b): the first direction. The renderer lives in its own allocation,
  `rustty_core::renderer_seam::RendererBox`. It is built with `Box::into_raw` right after
  `Renderer::init`, before `Thread::init`, and the render thread and `Surface.renderer` share that
  one raw root. A main-thread `&mut Surface` / `&Surface` now covers the pointer's bytes only. This
  fixes every main-thread entry at once, with no change to them: GTK `render_surface_resize` →
  `contentScaleCallback` / `sizeCallback`, the key, mouse, focus and clipboard callbacks, and the
  embedded and C-API entries. The main-thread renderer entries were already raw after N437; they now
  go through `RendererBox::as_ptr()`: `draw`, `displayRealized`, `displayUnrealized`, init's
  `attachHostView`, the GTK `takeFrame`, and the C API's `font_grid` read. `RendererBox::Drop` frees
  the box at the moment the inline field used to drop, so the renderer's own `Drop`s keep their
  timing, and `deinit` stays explicit. No change to `crates/rustty-renderer`. The rejected direction
  (raw-self main-thread entries) would touch every apprt-facing core method and all their `&mut self`
  helpers.
- **proof**: `.port/phase-2/mirihar-n441` (the real `Mutex` and `DrawMutex` by `#[path]`; 16 seeds,
  SB and TB). `n441_inline_renderer` (whole-`&mut Surface` callbacks vs the render thread's locked
  writes) is UB under both. `n441_boxed_renderer` (the fix, with a main-thread locked narrow renderer
  entry) is CLEAN under both. LIFETIMES row `src/Surface.zig Surface renderer`.
- **fix round 1** (P7-W7 T5b, ruling 43): round 0 freed the box in `RendererBox::drop`, which never
  runs. Every owner frees the core `Surface` without drop glue: embedded `closeSurface` through
  `ManuallyDrop`, GTK through `dealloc`, and the init-error callers through the `MaybeUninit` slot.
  So every closed surface leaked its renderer allocation. `RendererBox` now has no `Drop`;
  `RendererBox::dealloc_after_deinit` frees the allocation as `Box<ManuallyDrop<Renderer>>` (no drop
  glue, which after `deinit` would tear the renderer down twice). It runs right after
  `Renderer::deinit` in `Surface::deinit`, in `initErrdefers`, and at init's two pre-publication
  errdefers. Proof: `n441_box_drop_leak` (the round-0 shape, freed through `ManuallyDrop`) fails
  Miri's leak check under SB and TB; `n441_box_dealloc_after_deinit` is CLEAN under both.
- **fix round 2** (P7-W7 T5b, ruling 46): round 1 freed the box as `ManuallyDrop`, and
  `Renderer::deinit` left the backend to `Metal::drop`, which never ran.
  - The reference's `Metal.deinit` → `layer.release()` clears the layer's display callback (upstream
    4b4a5b241). In the port that clear lived only in `Metal::drop` / `IOSurfaceLayer::drop`.
  - So a view-retained layer (the `ExpiringUndoManager` keeps the view; `needsDisplayOnBoundsChange`
    is set) kept a callback whose context pointed into the freed renderer, and a resize or display
    pass ran `draw_frame_raw` on freed memory.
  - The callback context, queue, device, layer reference, search matches, `Arc`s and, on Linux, the
    EGL context also leaked on every close.

  Now `Metal::deinit(&mut self)` (idempotent; `Drop` calls it again) clears the callback and frees the
  context, and `Renderer::deinit` calls it where the reference calls `self.api.deinit()`.
  `RendererBox::dealloc_after_deinit` then drops the renderer normally. An audit showed every
  explicit `deinit` in `Renderer::deinit` (macOS and the OpenGL twin) leaves its field safe to drop
  (T5b report). The OpenGL twin's `api` teardown is `OffscreenContext`'s `Drop`, now run on the main
  thread as the reference's `OpenGL.deinit` is.
  Proof: `n441_r1_display_after_free` is UB and `n441_r1_drop_only_fields_leak` LEAKs under SB and
  TB; `n441_r2_deinit_then_drop` is CLEAN. The live Metal test
  `generic::macos_impl::tests::deinit_clears_the_layer_display_callback` fails without the
  `api.deinit()` call.
- **residual**: the same race class covers the other fields that other threads write through raw
  pointers into the surface: `io` (the inline terminal), `io_thread`, `renderer_thread` and
  `renderer_state`. That is **N444**, CLOSED in W7.T5c (the same own-allocation shape, plus `search.state`).

## N442 — `performBindingAction(&mut self)` holds a protected `&mut Surface` across the host callback that re-enters the same surface

- **id**: N442. P7-W8 T2 fix round 1 (review IMPORTANT, controller ruling 37). **severity**: medium
  (aliasing-model UB on every menu or keybind split/tab/window; no observed miscompile). **kind**: port
  soundness (Stacked Borrows; a function-argument protector). **Not upstream**: Zig's `*Surface` carries no
  aliasing rule and no protector, so the reference is sound. **state**: CLOSED (fixed, P7-W7 T5b).
- **state history**: opened OPEN at P7-W8 T2 fix round 1, owned by P7-W7 T5 (to land with N441); P7-W7
  T5b closed it.
- **shape**: `rustty_core::Surface::performBindingAction(&mut self, ..)` (`crates/rustty-core/src/surface.rs:11270`)
  is a `&mut self` method, so `self` is a protected Unique for the whole call. Its `new_split`
  (`:12537`), `new_tab` (`:12426`) and `new_window` (`:11282`) arms call `rt_app.perform_action(..)`, which
  runs the embedder's C action callback synchronously while that protector is live.
- **re-entrant reader**: the macOS host's action handlers call
  `rustty_surface_inherited_config(surface, RUSTTY_SURFACE_CONTEXT_{SPLIT,TAB,WINDOW})` inside that
  callback (`macos/Sources/RusttyKit/Rustty.App.swift:1004`, `:976`, `:940`). It reaches
  `embedded::Surface::newSurfaceOptions(&self)` (`crates/rustty-apprt/src/embedded.rs:2096`), which reads
  `self.core_surface.font_size` (and the working directory) through the parent embedded-surface pointer: a
  read through a tag the protected Unique invalidates, which is UB under Stacked Borrows.
- **routes** (all three reach `performBindingAction` with a `&mut` formed from the raw C pointer):
  - the keybind path, `rustty_surface_key` (`crates/rustty-apprt-c/src/lib.rs:1878`) → `keyCallback` →
    binding dispatch;
  - `rustty_surface_binding_action` (`lib.rs:2390`);
  - the menu path since a1bf5b5e1 (P7-W8 T2): `rustty_surface_split` (`lib.rs:2249`), and by the same shape
    `rustty_surface_split_focus` (`:2286`) and `rustty_surface_request_close` (`:2214`). Before the port of
    a1bf5b5e1 the menu split formed only a shared `&core_surface` for the `Target`, which a shared read
    does not invalidate.
- **reachability**: every menu or keybind split (Cmd-D), and every keybind or binding-action new tab and new
  window, on macOS. The GTK apprt does not call back into the core surface from its action handler this way.
- **test**: `crates/rustty-apprt-c/tests/macos_window_host.rs::split_and_close_exports_reach_the_host` —
  its `cb_action` calls `rustty_surface_inherited_config` on `NewSplit`, so the re-entry is exercised on a
  live surface. Miri cannot see it across the FFI; the test proves only that the re-entry runs.
- **fix direction**: raw-self dispatch (`*mut Surface` with narrow field access, memory
  `rust-mut-args-strongly-protected`) for the `performBindingAction` arms that call
  `rt_app.perform_action`, so no protected `&mut Surface` spans the host callback; the three routes then
  enter through the raw root. LIFETIMES row first (`.port/audits/GHOSTTY_LIFETIMES.tsv`, core `Surface`
  `[self across rt_app.perform_action]`), and an isolated harness under `.port/phase-2/mirihar-*` that is UB
  before (protected `&mut` + a re-entrant read through the parent pointer) and clean after, under SB and TB.
- **fix** (P7-W7 T5b): `Surface::performBindingAction`, `keyCallback` and `maybeHandleBinding` are
  raw-self (`unsafe fn(self_: *mut Surface, ..)`).
  - Each takes an unprotected local reborrow `this` for its ordinary work. That reborrow is dead
    before every call that can run the host callback and is taken afresh after it.
    `maybeHandleBinding` re-takes it lazily after the dispatch, so the closing check touches no
    surface byte.
  - The `new_window`, `new_tab` and `new_split` arms touch the surface only by place through `self_`,
    plus the shared `Target::Surface(&*self_)`. A read-only re-entry is sound under a shared
    protector.
  - `App::newWindow` is raw-self (fix round 1; round 0's `&self` was not enough). The host's
    handlers also create the new surface synchronously: the `NotificationCenter` observers run
    `TerminalController.newWindow` / `newSplit` → `SurfaceView` init → `rustty_surface_new` →
    `addSurface`, a push to the core app's `surfaces` under a shared protector. That is UB under SB
    and TB. See N443 for the app side.
  - The routes enter through the raw root: `rustty_surface_key` → `App::keyEvent` (raw-self since
    fix round 1) →
    `Surface::keyCallback(&raw mut (*surface).core_surface, ..)`, and `rustty_surface_binding_action`,
    `_split`, `_split_focus` and `_request_close` → `Surface::performBindingAction(&raw mut
    (*ptr).core_surface, ..)`. `App::performAllAction` and the GTK callers use the same associated
    fns.
  - Two provenance cleanups in the rewritten bodies. The leader arm reads the trigger before
    `encodeKey` reborrows the surface, because for the default set `entry.0` points into
    `this.config`. The search arm's `surface_ptr` is `self_`.
  - Behaviour is unchanged. The menu-shortcut parity harness passes (51 actions), and so does
    `split_and_close_exports_reach_the_host`.
- **audit** (which arms' host callbacks re-enter). The macOS host's action handlers
  (`Rustty.App.swift`) call back into a surface only in `newWindow`, `newTab` and `newSplit`, which
  call `rustty_surface_inherited_config`. The split tree's focus moves are `DispatchQueue.main.async`.
  The GTK apprt's action handlers do not call back into the core surface. A host that WRITES the
  target surface inside an action callback would still conflict with the shared `Target` protector.
  None was found; the one candidate, `syncSurfaceTreeOcclusionState`'s `rustty_surface_set_occlusion`,
  fires only for a view whose cached visibility differs.
- **proof**: `.port/phase-2/mirihar-n441`. Since fix round 1, the host model re-enters the way the
  macOS host does: `inherited_config`, then `rustty_surface_new` (a push to the app's surface list),
  with the host's own app pointer.
  - `n442_shipped_key_route` and `n442_shipped_binding_new_window` are UB under SB and TB.
    `n442_shipped_binding_new_{split,tab}` are UB under SB.
  - The round-0 shapes are still UB: `r0_binding_new_window` (`App::newWindow(&self)`) under SB and
    TB, and `r0_key_route_new_split` (embedded `keyEvent(&self)` plus `newSurface(&mut self)`) under
    SB.
  - The negative `n442_raw_this_spans` (a local reborrow used after the callback) is UB under both.
  - The fix, `fixed_key_route_all` and `fixed_binding_all`, is CLEAN under both.
  - LIFETIMES row `src/Surface.zig Surface [self across rt_app.performAction]` is updated.
- **known limits**:
  - The shared `Target::Surface(&C)` passed to `perform_action` is a protector over the target
    surface for the whole host callback. A host that WRITES that surface synchronously inside an
    action callback would still be UB. The macOS host does not (see the audit), and no apprt in the
    tree does.
  - `rustty_surface_font_grid`'s place read of the renderer's `font_grid`
    (`crates/rustty-apprt-c/src/lib.rs`) takes no lock, exactly as the reference's
    `ptr.core_surface.renderer.font_grid`. It is as racy as the Zig original against a render-thread
    `set_font_grid`; not changed here.
- **surface freed during a close binding** (N-only, not a UD). A closing binding's host callback could
  in principle free the surface while `close(&mut self)` / the key path is still on the stack. It
  does not happen synchronously in either shipped apprt.
  - macOS: the close notification's handler replaces the split tree, but the `ExpiringUndoManager`
    keeps `oldTree` (and so the view) alive for the undo timeout. Even with timeout 0, the
    SwiftUI-hosted view lives until the next update pass, so `Rustty.Surface`'s deinit (and
    `rustty_surface_free`) runs after the key event has returned.
  - GTK: `close` emits `close-request` with `g_signal_emit`, which holds a reference on the instance
    for the emission. GTK4 event propagation also holds a reference on each widget whose controllers
    it runs. The core surface is freed only in the widget's finalize (`imp::Surface::drop` →
    `finalize_core_surface`), which runs at the last unref, after both have returned.
- **residual**: the same protector shape on the core `App` is **N443**, CLOSED by fix round 1.
- **residual (P7-W12 T1)**: the close callback itself re-enters the surface (it does not free it):
  `close(&mut self)` was protected across `rt_surface.close`, and the macOS close notification can
  write the surface through `rustty_surface_deny_clipboard_request`, while GTK's `close-request`
  handler reads it (`needsConfirmQuit`). That is **N465**, CLOSED.

## N443 — `App::performAction`/`keyEvent` hold a protected reference to the app across any host callback that re-enters it

- **id**: N443. P7-W7 T5b (the N442 audit), widened by fix round 1 (ruling 43). **severity**: medium.
  It is aliasing-model UB on every app-scoped keybind whose host callback writes the app, and the
  writes make it UB under Tree Borrows too; no miscompile has been observed. **kind**: port soundness
  (a function-argument protector). **Not upstream**: Zig's `*App` has no protector. **state**: CLOSED
  (fixed, P7-W7 T5b fix round 1).
- **shape**: every core `App` entry that runs `rt_app.perform_action` held `&mut self` (or `&self`)
  across it, and the macOS host re-enters the app synchronously:
  - `reload_config` (the default `super+shift+,`): the handler calls `rustty_app_update_config` →
    `updateConfig(&mut self)`, which writes the app and every surface.
  - `new_window` (from `performAction` or a surface's binding): the observers create the surface,
    `rustty_surface_new` → `addSurface`, a push to `surfaces`.
  - `close_all_windows` and `quit` (`NSApplication.terminate` → `applicationShouldTerminate` →
    `rustty_app_needs_confirm_quit`).
  - The same holds on the entries that reach those arms: `tick` → `drainMailbox`, `keyEvent`,
    `performAllAction`, `performAllChainedAction` and `colorSchemeEvent` (its soft `reload_config`).
  - The embedded app has the same shape: `App::keyEvent(&self)` stayed protected across
    `keyCallback`, and `newSurface(&mut self)` retagged the app under `perform_action(&self)`'s
    shared protector.
- **fix** (P7-W7 T5b fix round 1): raw-self throughout.
  - The core `App`: `tick`, `drainMailbox`, `newWindow`, `keyEvent`, `performAction`,
    `performAllChainedAction`, `performAllAction`, `colorSchemeEvent`, `closeSurface` and
    `surfaceMessage` are `unsafe fn(self_: *mut App, ..)`. They reach the app only by place, in short
    reborrows that never span a host callback.
  - The embedded app: `keyEvent(self_: *const App, ..)` and `newSurface(self_: *mut App, ..)`, the
    latter storing `self_` as the surface's app back-pointer.
  - All callers were updated: `rustty_app_tick`, `rustty_app_key`, `rustty_surface_key`,
    `rustty_surface_new`, `rustty_app_set_color_scheme`, the GTK tick, color-scheme and global-action
    sites, `performBindingAction`'s app arms, and `maybeHandleBinding`'s global dispatch.
  - `rt_app: &A` stays a shared reference: `perform_action` is a `&self` trait method. It is sound
    because nothing writes the embedded app's non-`UnsafeCell` bytes during a callback once
    `newSurface` is raw-self.
- **proof**: `.port/phase-2/mirihar-n441`.
  - The shipped and round-0 shapes are UB: `r0_binding_reload_config`,
    `r0_app_tick_reload_config` and `r0_app_key_global_reload_config` under SB and TB, and
    `r0_binding_quit` under SB.
  - The fix, `fixed_binding_all`, `fixed_app_tick_all` and `fixed_app_key_global_all` (reload_config,
    quit, new_window, new_split and new_tab), is CLEAN under both.
  - LIFETIMES row `src/App.zig App [self across rt_app.performAction]`.

## N444 — the io thread's and render thread's inline state in the core `Surface` is covered by main-thread whole-`Surface` references

- **id**: N444. P7-W7 T5b (found while fixing N441). **severity**: medium. It is aliasing-model UB
  whenever a main-thread surface callback overlaps io, render or search thread work, which happens
  constantly; no miscompile has been observed. **kind**: port soundness (Stacked and Tree Borrows).
  **Not upstream**: Zig's `*Surface` has no aliasing rule, so no UD. **state**: CLOSED (fixed,
  P7-W7 T5c).
- **finding**: N441's race class, beyond the renderer. The core `Surface` embedded by value five
  blocks that other threads write through raw pointers into the surface:
  - `io: Termio`. The io read thread writes `io.terminal` under `renderer_state.mutex`
    (`processOutput`), and the render thread reads it.
  - `io_thread` and `renderer_thread`. Their own threads write them (loop state).
  - `renderer_state`. The io and render threads bump its D4 handoff atomics (`demand`,
    `handoff_gen`); the render thread reads it under the mutex; the main thread writes it.
  - `search.state` (found by T5c). The search thread writes its own loop state.
  - The search callback (fix round 1, ruling 48). `searchCallback_` got the surface as its userdata
    and read `renderer_thread`, `rt_app` and `app` through it on the search thread: non-atomic reads
    of `Surface` bytes, racing a main-thread `&mut Surface` retag under SB.

  Every main-thread `&mut Surface` / `&Surface` retag (the same apprt entries N441 lists) covers
  those bytes, so it races with those accesses. Two narrower main-thread shapes had the same class:
  `queueIo` formed `&mut Termio` (`self.io.queueMessage(&mut self)`), and `Mailbox::send(&mut self)`
  popped the io writer thread's drain tag under SB. `getForegroundPid` / `getTtyName` also formed
  `&mut Termio`.
- **fix** (P7-W7 T5c): the N441 shape for every block, plus narrow main-thread io entries.
  - `rustty_core::thread_shared::ThreadSharedBox<T>`: each block in its own allocation, shared with
    its thread through one raw root. The main thread reaches it by place through `as_ptr()`, locked
    where the reference locks. The `Surface` fields keep their names and order.
  - `Surface::init` allocates the `renderer_thread` / `renderer_state` slots uninitialized before
    `Renderer::init` (their addresses are handed out first), and `io` uninitialized at the big write
    (`renderer_state.terminal` takes `&self.io.terminal` while io is undefined, as the reference
    does). `Termio::init` writes it in place.
  - `rustty_termio::Termio::queueMessage`, `getForegroundPid` and `getTtyName` are raw-self (the
    reference's `self: *Termio`). `Mailbox::send` / `notify` take `&self`, since every callee is
    `&self`, and the stream handler sends through `&Mailbox`.
- **teardown**: there is no `Drop`. Every path frees each new allocation exactly once:
  - close → `Surface::deinit`: each block torn down as it was inline, then `dealloc` (no glue; the
    inline field was never dropped either);
  - `Renderer::init`, `renderer::Thread::init` and `termio::Thread::init` failures: the unwritten
    `renderer_thread` / `renderer_state` slots are `dealloc`ed;
  - `initErrdefers` (both `io_live` values): every block `dealloc`ed, io whether or not initialized;
  - the owners' free of the surface itself is unchanged (embedded `ManuallyDrop`, GTK `dealloc`, the
    `MaybeUninit` slot).
  - `Search.state` keeps its normal drop after `state.deinit()` (`drop_and_dealloc`), because the
    inline field was dropped with its `Search`.
- **fix round 1** (ruling 48): the search callback's userdata is a boxed `SearchCallbackCtx<A>`
  captured on the main thread at spawn (the renderer mailbox pointer, the renderer wakeup pointer, a
  copy of the surface mailbox). `searchCallback_` reads only it, never the surface. It is freed
  exactly once: by `Search::deinit` after the join, on the search `Thread::init` error, and on the
  spawn failure.
- **proof**: `.port/phase-2/mirihar-n444` (16 seeds, SB and TB).
  - Shipped: `n444_inline_io_terminal`, `n444_inline_io_thread`,
    `n444_inline_renderer_thread_state`, `n444_inline_renderer_state_atomics` and
    `n444_inline_search_state` are UB under both; `n444_boxed_queue_io_termio_mut` is UB under both;
    `n444_boxed_queue_io_send_mut` is UB under SB.
  - Fixed: `n444_boxed_all` (every writer thread against every main-thread entry) is CLEAN under both.
  - Teardown: `n444_teardown_all_paths` is CLEAN under both; the negative arms `n444_drop_box_leak`
    and `n444_init_error_io_slot_leak` LEAK.
  - Fix round 1: `n444_search_cb_reads_surface` (the round-0 callback) is UB under SB and CLEAN under
    TB; `n444_search_cb_ctx` is CLEAN under both. LIFETIMES row
    `src/Surface.zig Surface [self as search event_userdata]`; live test rustty-apprt-c
    `macos_window_host::search_callback_context_lives_for_the_search_thread`.
  - The superseded direction proof `.port/phase-2/mirihar-n441` `n441_residual_terminal_*` stays.
  - LIFETIMES rows `src/Surface.zig Surface io / io_thread / renderer_thread / renderer_state`,
    `src/Surface.zig Search state` and `src/termio/Termio.zig Termio [self in queueMessage]`.
- **tests**: the macOS core, apprt, apprt-c and termio suites; the menu-shortcut parity harness; on
  Linux the GTK build, the core, apprt and termio suites and the live `gtk_window_live` and
  `gtk_first_light` tests. The fabricated-surface tests (`surface_visibility_kat`,
  `clipboard_codepoint_map_fixture_is_miri_clean`, `surface_mok2_keypad_alt_ctrl_end_to_end`,
  `queueIo_frees_allocated_writes_in_readonly_mode`) write and free the boxes and pass under Miri's
  leak check.
- **limit**: N449 below (the io writer thread's own `&mut Termio`) is independent of the surface
  layout; it was OPEN here and is CLOSED by P7-W7 T5d.

## N449 — the io writer thread's `&mut self` `Termio` methods cover the terminal the read thread writes

- **id**: N449. P7-W7 T5c (found while proving N444). **severity**: medium. It is aliasing-model UB
  whenever an io-thread message (focus, resize, config change, size report, clear screen, scroll,
  write, ...) overlaps pty output, which is common; no miscompile has been observed. **kind**: port
  soundness (Stacked and Tree Borrows). **Not upstream**: Zig's `*Termio` has no aliasing rule.
  **state**: CLOSED (P7-W7 T5d).
- **finding**: the io writer thread's `drainMailbox` calls `&mut self` `Termio` methods:
  `focusGained`, `resize`, `changeConfig`, `sizeReport`, `clearScreen`, `scrollViewport`,
  `jumpToPrompt`, `colorSchemeReport`, `visibilityReport`, `queueWrite`, `kittyClipboardGrant`,
  `resetSynchronizedOutput` and `threadExit`. Each is a protected Unique over the whole `Termio`,
  terminal included. Meanwhile:
  - the io read thread writes the terminal under `renderer_state.mutex` through `processOutput`
    (raw-self);
  - the main and render threads reach the terminal by place under the mutex;
  - the main thread reads `mailbox` (`queueMessage`) and `backend` (`getForegroundPid`,
    `getTtyName`, the close-confirm command read) by place.

  A writer-thread retag of the whole `Termio` races with each of them. This is independent of where
  the `Termio` lives, so N444's fix does not cover it.
- **proof**: `.port/phase-2/mirihar-n444`: `n449_writer_termio_mut` is UB under SB and TB (a data race
  between the writer's retag and the read thread's terminal write); `n449_writer_termio_raw`, the
  same method raw-self with narrow access, is CLEAN under both. LIFETIMES row
  `src/termio/Termio.zig Termio [self in io writer-thread methods]`.
- **fix direction**: the `processOutput` shape. Make those methods raw-self
  (`unsafe fn(self_: *mut Termio, ..)`), reaching only the fields each touches, by place, and never
  holding a whole-`Termio` reference across the renderer-mutex sections or a backend call. The
  backend's own `&mut` receivers (`Exec`, `Subprocess`, the pty's cached tty name) need the same audit.
- **fix** (P7-W7 T5d, no behaviour change, no UD):
  - `crates/rustty-termio/src/termio_impl.rs`: `threadExit`, `queueWrite`, `changeConfig`, `resize`,
    `sizeReport(Locked)`, `resetSynchronizedOutput`, `clearScreen`, `scrollViewport`,
    `jumpToPrompt`, `focusGained`, `kittyClipboardGrant`, `colorSchemeReport(Locked)` and
    `visibilityReport(Locked)` are raw-self and reach only the fields the reference touches, by
    place. The terminal, stream handler and config are borrowed only inside the renderer-mutex
    section; `changeConfig` borrows the terminal only after the stream handler has written it through
    its own pointer. `crates/rustty-termio/src/thread.rs` calls them as `Termio::m(io, ..)`.
  - the backend chain: `Backend` is `#[repr(transparent)]` and `Backend::exec_ptr` is the
    `.exec => |*exec|` projection as a pointer cast; the cross-thread `Backend`, `Exec` and
    `Subprocess` methods are raw-self (`backend.rs`, `exec.rs`).
  - `crates/rustty-os/src/pty.rs`: `setSize` / `getForegroundPid` / `getTtyName` take `&self` on every
    pty; the `PosixPty` tty-name cache is `UnsafeCell` / `Cell`, the `WindowsPty` size a `Cell`.
  - `crates/rustty-core/src/surface.rs` `childExitedAbnormally`: the close-confirm command read
    borrows `subprocess.args` only, through `Backend::exec_ptr`.
- **fix proof**: `.port/phase-2/mirihar-n444` (16 seeds, SB and TB, `bash run-all.sh`):
  `n449_shipped` UB/UB, `n449_main_backend_mut` UB/UB, `n449_pty_set_size_mut` UB/UB,
  `n449_close_confirm_backend_ref` UB/UB, `n449_change_config_terminal_early` UB/UB, `n449_fixed`
  CLEAN/CLEAN; every N444 row is unchanged. LIFETIMES rows: `src/termio/Termio.zig Termio [self in io
  writer-thread methods]` (FIXED), `src/termio/backend.zig Backend`, `src/termio/Exec.zig Exec` and
  `Subprocess` `[self in cross-thread methods]`, `src/pty.zig PosixPty tty_name_buf / tty_name`.
  Live: `RUSTTY_REQUIRE_METAL=1 cargo test -p rustty-termio -p rustty-core -p rustty-apprt -p
  rustty-apprt-c -p rustty-os` on macOS; on Linux (rvm) the termio / core / apprt suites with GTK and
  the live `gtk_window_live` / `gtk_first_light`.
- **residual (not N449)**: `Exec::threadEnter` still runs `subprocess.start(&mut Subprocess)` on the io
  thread, which publishes `pty` / `process`. A main-thread `getProcessInfo` in that window reads
  `pty` while it is written; that is a plain data race in the reference too (not an aliasing-model
  artefact). Ruling 52 confirmed it: now **UD367**, fixed in T5d fix round 1 (`Exec.pty_ready`). The
  same review found **UD368** (`Termio.resize` stores `size` before the renderer mutex while the read
  thread reads it under the mutex), also fixed in fix round 1.

## N448 — superseded: now UD366

- **id**: N448. P7-W7 T5b fix round 1 (found while making `App::performAllAction` raw-self).
  **state**: superseded by **UD366** (ruling 46: confirmed as an upstream defect). The record,
  mechanism, fix and tests are in `.port/parity-closure/UPSTREAM-DEFECTS.md` UD366. This entry stays
  only as a pointer, because the T5b fix-round-1 commits, the harness and the LIFETIMES row cite
  N448.

## N450 — the live inspector KAT leaves both render threads running when an assertion panics, and the process then crashes at exit

- **id**: N450. P7-W7 T6 (controller ruling 47). **severity**: low. Only the test harness is
  affected; no product path is involved. **kind**: test-harness teardown order (a crash at process
  exit after a failed assertion). **Not upstream**: `gtk_inspector_dump` is a Rustty KAT, and the
  reference has no counterpart.
- **rustty (pre-fix)**: `crates/rustty-apprt/tests/gtk_inspector_dump.rs`. `inspector_dump` stopped
  its two render threads only through the trailing `gtk_live::stop_render_thread` loop, which runs
  after `app.run()`. An assertion that panicked earlier skipped that loop.
- **finding**: libtest catches the panic, prints the result and calls `exit`. The two `renderer`
  threads are still in their xev loop at that point; gdb, breaking on `exit`, shows both threads plus
  the `io` threads alive. Mesa's exit-time teardown then runs under them. The crash occurred in 3 of
  8 runs of the pre-fix binary on rvm:
  - SIGSEGV on a `renderer` thread in `glReadPixels`
    (`OpenGL::present` → `Target::read_pixels_alloc` → `gl::draw::read_pixels`, from
    `Frame::complete` → `draw_frame_locked`, in coredumpctl);
  - once as a glibc `corrupted double-linked list` abort.

  The llvmpipe worker threads exit just before the fault, which is the driver tearing down.
- **how found**: the P7-W7 T5c report, which saw "SIGSEGV after the panic". It was reproduced with
  the floating headless-sway launch, which always fails assertion (c); that assertion failure is a
  recipe issue, not a defect (see below).
- **fix**: a new `gtk_live::RenderThreadGuard`. It is declared after the core app and the apprt app,
  so unwinding drops it first, while the core app is alive. Its `Drop`:
  - sends each surface's render thread its stop;
  - drains the core-app mailbox while it waits, because a `.forever` push from `frameCompleted`
    must not block the join;
  - joins the thread, and never panics.

  `inspector_dump` uses the guard on both the pass path and the panic path.
- **fail-first**: under the floating recipe, the pre-fix binary crashed in 3 of 8 runs (SIGSEGV
  ×2, abort ×1). The fixed binary crashed in 0 of 16 runs: all 16 failed cleanly with exit 101 and
  the new per-terminal diagnostic `[false, false]`.
- **related, not a defect**: assertion (c), "both terminals kept presenting frames alongside the
  inspector", fails only when the sway config floats windows (the v20b gate's `floating enable`
  rule).
  - sway centers floating windows, and the 1000x600 inspector then fully covers both terminals.
  - A fully covered surface gets no `wl_surface.frame` callbacks, so GTK never snapshots its
    `RenderSurface` and its texture never changes.
  - The result is the same at `99f23b5ee` (T2, where the assertion was introduced), at `19cb14955`
    (T5b) and at `f00301f28`: the test passes when windows tile and fails when they float.
    Moving the inspector off the terminals over IPC makes the floating launch pass.
  - The canonical recipe is now `.port/tools/gtk_live_headless_sway.sh` (tiling), cited from the
    test headers.
- **state**: CLOSED (P7-W7 T6, extended by Ruling 50). Every live KAT now uses the guard:
  `gtk_inspector_dump`, `gtk_first_light`, `gtk_window_live`, `gtk_glyph_dump`, `gtk_input_focus`,
  `gtk_render_surface` and `gtk_split_drop`.
- **extension (Ruling 50)**. The live KATs now close their surfaces the way the app does and tear
  the apps down in `run_gtk`'s order (`terminate`, then `destroy`). Three findings came out of that
  work:
  - **The termio `Subprocess` "double-free" is gone.** Five KATs `forget` their surfaces to avoid
    a `free(): invalid size` abort in `Subprocess`'s `Drop`. The comment describing it dates from
    `269845ec9`, and `f3bb00b5c` (consuming `Subprocess::deinit`) and `6fe5259bf` (GTK finalize
    frees the `CoreSurface` box without drop glue) fixed the abort the same day. The workaround
    and its comments were stale and are now removed.
    - Each KAT now asserts, through `gtk_live::close_surface(s)`, that the `Surface` GObject was
      finalized and its shell killed and reaped.
    - This held for all 10 runnable live KATs on rvm under the canonical recipe, and for 20 more
      repeat runs.
    - No N451 is needed, because nothing was live.
  - **A leftover app-created window crashed two KATs at exit, after they passed.**
    `App::run` emits `activate`, and its `new_window_thunk` opens a second window with its own
    terminal.
    - `gtk_glyph_dump` and `gtk_input_focus` left that window's render thread running at exit,
      and exited with SIGSEGV in 2 of 3 passing runs. This is the same mechanism as above, and a
      `gtk_glyph_dump` core dump from W7b has the same shape.
    - `gtk_live::close_other_windows` now closes every remaining toplevel, and asserts that each
      `Surface` in it was finalized and its shell reaped.
    - After the fix, all 20 repeat runs exited 0. Without that close, `CoreApp::deinit`'s
      font-grid `debug_assert` fired.
    - The guard's `Drop` also stops the render threads of surfaces in any toplevel still open.
  - **`gtk_render_surface` left `rt_app` pointing at a moved value (test harness).** `boot()`
    bound `rt_app` to a local `gtk::App` and then moved it into `Live`.
    - The first real close, `deleteSurface` → the `quit_timer` action, read the stale address and
      aborted with "misaligned pointer dereference" in `ObjectSubclassExt::from_obj`
      (`application.rs` `action::quit_timer`).
    - `Live.app` is now boxed, so the bound address stays stable.
    - The product is not affected, because `gtk::App::run` re-binds `rt_app` to its own `self`.
- **fail-cleanly check (Ruling 50)**. A temporary rvm-only patch panicked at the start of
  `close_surfaces`, with every thread live. All seven close-path KATs then exited 101 twice each,
  with no SIGSEGV and no abort.

## N456 — host action callbacks that resize the window write the target surface under the shared `Target` protector (`reset_window_size` live, `resize_window` latent)

- **id**: N456. P7-W6 T1 (the port of 714fe9b90); renumbered from N451 and widened by fix round 1
  (Ruling 55). **severity**: medium. It is aliasing-model UB today on every `reset_window_size`
  keybind on macOS; no miscompile has been observed. **kind**: port soundness (Stacked Borrows and
  Tree Borrows; a function-argument protector). **Not upstream**: Zig's `*Surface` has no protector.
  **state**: CLOSED (fixed, P7-W6 T3); was PARTIAL (T1). Widened by T3 fix round 1 (ruling 57) to the
  colour-scheme soft-reload chain, also fixed there. The residual receiver protectors are **N459**.
- **shape**: an action whose host handler resizes the window writes the core surface inside the
  callback. That write happens while `perform_action`'s shared `apprt::Target::Surface(&Surface)`
  argument protects the whole surface.
  - `reset_window_size` (**live**):
    - core: `performBindingAction`'s arm passes `Target::Surface(&*this)`
      (`crates/rustty-core/src/surface.rs:13700-13705` after T3; `:13606` at T1).
    - Swift: `Rustty.App.swift:2053` `resetWindowSize` posts a notification, and NotificationCenter
      runs its observers synchronously. `onResetWindowSize` → `returnToDefaultSize` →
      `apply(to:)` → `window.setFrame(rect, display: true)` (`TerminalController.swift:1762`).
    - The frame change lays the surface view out synchronously (`SurfaceScrollView.layout` →
      `synchronizeCoreSurface` → `SurfaceView.sizeDidChange` → `rustty_surface_set_size` →
      `sizeCallback`), which writes the core surface. That conflicts with the shared protector under
      both SB and TB.
  - `resize_window` (**latent** until P7-W9 T2 ports the Swift handler):
    - `Surface.resizeWindow` (from `handleMessage(.resize_window)`, a program's CSI 8 t) performs
      `.resize_window`.
    - The macOS handler (`TerminalController.resizeWindow`, 714fe9b90) calls
      `window.setFrame(frame, display: true)` in the callback, which is the same write.
- **P7-W6 T1 fix (partial)**: raw-self, as N442.
  - `Surface::handleMessage(self_: *mut Surface, ..)` is an unsafe associated fn. It sends
    `.resize_window` to `resizeWindow(self_: *mut Surface, ..)`, which reads `config`, `rt_surface`,
    `size` and `rt_app` by place in short reborrows that end before `perform_action`.
  - Every other arm runs in `handleMessageArms(&mut self)`, unchanged.
  - Callers pass the pointer: `App::surfaceMessage` (raw-self) and `App::updateConfig`.
  - LIFETIMES rows: `src/Surface.zig Surface [self across rt_app.performAction: handleMessage
    .resize_window]` (new), and the N442 row's residual (amended).
- **decision (Ruling 55)**: make `Target::Surface` a raw pointer (`*const`/`NonNull`: no protector,
  the reference's `*Surface` semantics). The alternative, deferring the Swift `setFrame` with
  `DispatchQueue.main.async`, is rejected as a behaviour change. This lands as **P7-W6 T3**, before
  P7-W9 T2's Swift `resize_window` handler.
- **test**: `crates/rustty-core/src/surface.rs` `resize_window_tests::csi_8_t_resizes_the_window_only_when_allowed`
  and `ud371_csi_8_t_overflow_returns_without_panicking` drive `Surface::handleMessage` through the raw
  root on a fabricated surface. Miri clean (SB), macOS 2026-09-29. Their test host does not re-enter,
  so they cover the raw-self half only; the protector half is for T3's harness.
- **fix** (P7-W6 T3, ruling 55): `rustty_apprt_api::Target::Surface` carries a
  `SurfacePtr<'s, C>` (`NonNull<C>` + `PhantomData<&'s C>`, re-exported from the crate root), not a
  `&'s C`. It has no protector, like the reference's `*CoreSurface`.
  - Constructors: `Target::surface_ref(&C)` (the reference is dead at return; used at the `&self` /
    `&mut self` / local-reborrow `this` sites) and `unsafe Target::surface_ptr(*const C)` (the raw-self
    sites keep the raw root's provenance: `resizeWindow`, `App::newWindow`'s parent, the search and
    `performBindingAction` `self_` arms, `rustty_surface_split_resize` / `_split_equalize`, the GTK
    `new-tab` IPC).
  - Consumers read only `rt_surface`, through `SurfacePtr::rt_surface_ptr` (a transient `&C` for the
    one trait call): `Target::cval`, the embedded `performPreAction` `set_title` arm, the GTK
    `widget()` / `action_render` resolution. No consumer holds a reference across a host call.
  - Every core, apprt-embedded, apprt-c, GTK and test constructor was updated (about 90 sites). The C
    projection `rustty_target_s` / `TargetC` is unchanged: the embed-ABI gates pass, and
    `target_surface_is_a_raw_pointer_to_the_core_surface` pins its size and the projected pointer.
  - No behaviour change, no UD.
- **proof**: `.port/phase-2/mirihar-n456` (16 seeds, SB and TB). `n456_shipped_*` (key-route and
  binding-action `reset_window_size`, tick-route `resize_window`) are UB under both;
  `n456_fixed_ref_held_*` (the raw target, but a whole `&C` held across the callback) are UB under
  both; `n456_fixed_*` and `n456_fixed_all` are CLEAN under both. Live route:
  `crates/rustty-apprt-c/tests/macos_window_host.rs::reset_window_size_host_callback_resizes_the_target_surface`
  (its action callback calls `rustty_surface_set_size` on the target surface on `RESET_WINDOW_SIZE`;
  passes with `RUSTTY_REQUIRE_METAL=1`). `target_surface_is_a_raw_pointer_to_the_core_surface`
  (apprt-api) is Miri clean under SB and TB. LIFETIMES row
  `src/apprt/action.zig Target surface`.
- **widening, fix round 1 (ruling 57): the colour-scheme soft-reload chain** (live on macOS and GTK).
  - **shape**: `colorSchemeCallback(&mut self)` (embedded and core) → `notifyConfigConditionalState(&mut
    self)` performs `ReloadConfig { soft: true }` on the surface while both receiver protectors are
    live. The raw target alone does not help.
    - macOS: `rustty_surface_set_color_scheme` (every system appearance change,
      `BaseTerminalController.swift:1535`). The host's `configReload` → `reloadConfig(surface:soft:)`
      (`Rustty.App.swift:164-168`) calls `rustty_surface_update_config` →
      `(*surface).core_surface.updateConfig(..)` synchronously, a write to the same surface.
    - GTK: `handleStyleManagerDark` → `color_scheme_thunk`, per surface; `reload_config_inner` →
      `(&mut *core).updateConfig(..)` on the target.
  - **fix**: raw-self throughout.
    - `rustty_surface_set_color_scheme` → embedded `Surface::colorSchemeCallback(self_: *mut, ..)` →
      core `Surface::colorSchemeCallback(self_: *mut, ..)` (a local reborrow `this`, dead across the
      notify and taken afresh for `queueIo`) → `notifyConfigConditionalState(self_: *mut)` (`rt_app`
      by place, `Target::surface_ptr(self_)`).
    - The GTK thunk calls `rustty_core::surface::Surface::colorSchemeCallback(core, ..)`.
  - **proof**: `.port/phase-2/mirihar-n456`. `n456_t3_{macos,gtk}_color_scheme` (the T3 shape: raw
    target, `&mut` receivers) are UB under SB and TB; `n456_fixed_{macos,gtk}_color_scheme` are CLEAN
    under both. Live:
    `macos_window_host::color_scheme_soft_reload_host_callback_updates_the_target_surface` (its
    `cb_action` calls `rustty_surface_update_config` on `RELOAD_CONFIG` with a surface target; entered
    through `rustty_surface_set_color_scheme`). LIFETIMES row
    `src/Surface.zig Surface [self across rt_app.performAction: colorSchemeCallback]`.
  - Also in fix round 1: `presentSurface` is raw-self, dispatched from the raw `handleMessage` (see
    N459).
- **known limits** (the `Target` is raw, but these methods still hold their own `&mut self` / `&self`
  receiver protector across `perform_action`). The audit is of the Swift action handlers in
  `Rustty.App.swift` and of their notification observers, plus the GTK handlers (N442's audit):
  - **audited as not re-entrant** (the host handler writes no surface synchronously):
    - the `handleMessageArms` arms: `set_title`, `pwd`, mouse shape and visibility, clipboard,
      `child_exited`, bell, colour change, renderer health, scrollbar, search, progress, and the
      rest. None of their Swift handlers calls a `rustty_surface_*` export; they set `SurfaceView`
      properties or post notifications.
    - `setCellSize`: `cell_size`, handled with `DispatchQueue.main.async`.
    - `updateConfig`: `config_change`. The observers post `rusttyConfigDidChange`;
      `syncAppearance` calls only the app-level `rustty_set_window_background_blur` or defers with
      `DispatchQueue.main.async`.
    - `mouseRefreshLinks`: `mouse_over_link` sets `hoverUrl`.
    - `redraw`: `render`. It is a GTK `queue_draw`; the macOS host does not re-enter.
    - `showDesktopNotification`: `UNUserNotificationCenter`.
    - embedded `queueInspectorRender`: `render_inspector` posts `inspectorNeedsDisplay`.
  - **unverified**, recorded as **N459**:
    - `presentSurface` (`present_terminal`): `makeKeyAndOrderFront` may fire the focus publishers
      synchronously (`SurfaceView.swift:40`). Made raw-self in fix round 1 (cheap: one
      `handleMessage` arm), so it is no longer a limit.
    - `recomputeInitialSize` (`initial_size`): the handler sets `surfaceView.initialSize`, which the
      SwiftUI `idealWidth` binding reads. Whether that lays out and re-enters synchronously is not
      verified here. Its callers (`setCellSize` ← `setFontSize` ← many `&mut self` paths) make
      raw-self a wide change, so it was left protected. N459 is CLOSED (P7-W12 T0): no host
      re-enters the surface there, so no raw-self change is needed and it is no longer a limit.

## N459 — `recomputeInitialSize` holds a protected `&mut Surface` across the `initial_size` host callback (re-entry unverified)

- **id**: N459. P7-W6 T3 fix round 1 (ruling 57, review minor 2). **severity**: low. It is
  aliasing-model UB only if the macOS `initial_size` handler re-enters the surface synchronously, which
  is unverified; no miscompile has been observed. **kind**: port soundness (a function-argument
  protector). **Not upstream**: Zig's `*Surface` has no protector. **state**: CLOSED by P7-W12 T0
  (not re-entrant; evidence below).
- **shape**:
  - `Surface::recomputeInitialSize(&mut self)` performs `initial_size` with the raw target (N456).
    Its receiver is a protected Unique across the callback.
  - The callers are `(*self_).recomputeInitialSize()` in init (a `&mut` receiver formed from the raw
    root) and `setCellSize(&mut self)`, which is called from `setFontSize` (the font-size bindings,
    `updateConfig`, content-scale changes).
  - The macOS handler (`Rustty.App.swift:2034` `setInitialSize`) sets `surfaceView.initialSize`, a
    property the SwiftUI view's `idealWidth` / `idealHeight` read. If that assignment synchronously
    re-lays the view out (`SurfaceView.sizeDidChange` → `rustty_surface_set_size` →
    `sizeCallback`), it writes the protected surface: UB under SB and TB.
- **the second unverified case, `presentSurface`** (`present_terminal`: `makeKeyAndOrderFront` may fire
  the focus publishers synchronously, `SurfaceView.swift:40`), is FIXED in fix round 1: it is
  dispatched raw-self from `Surface::handleMessage` with `Target::surface_ptr(self_)`, like
  `resize_window`.
- **to do (P7-W12 T0)**:
  - Verify the `initial_size` re-entry, for example with a breakpoint on `rustty_surface_set_size`
    under `setInitialSize` in the running app.
  - If it is re-entrant, make `recomputeInitialSize` and its callers raw-self (`setCellSize`,
    `setFontSize` and their `&mut self` callers), and prove it on `.port/phase-2/mirihar-n456`.
  - If it is not re-entrant, close N459 with the evidence.
- **closed (P7-W12 T0): no host re-enters the surface under `recomputeInitialSize` or its callers.**
  Traced statically in the Swift and GTK sources:
  - macOS `initial_size`: `Rustty.App.swift:2037` `setInitialSize` calls `surfaceView(from:)`
    (`:580`, `rustty_surface_userdata`, which reads the embedded `Surface.userdata` field, outside the
    core surface's bytes, like every surface-targeted action) and assigns `surfaceView.initialSize`.
    That is a plain stored `var initialSize: NSSize?` (`SurfaceView_AppKit.swift:121`): no `didSet`,
    not `@Published`, so the assignment runs no code and sends no `objectWillChange` (the class is an
    `ObservableObject` through `OSSurfaceView`, but only `@Published` properties publish). The
    `idealWidth`/`idealHeight` reads (`TerminalView.swift:104-105`) and `TerminalController`'s
    `initialContentSize`/`defaultSize` reads (`:1119`, `:1810`) happen on SwiftUI's or AppKit's own
    later passes, not inside the setter. So no `sizeDidChange` → `rustty_surface_set_size` can run
    while the `&mut self` receiver is protected.
  - macOS `cell_size` (the other host callback on the `setFontSize` → `setCellSize` →
    `recomputeInitialSize` chain): `setCellSize` (`Rustty.App.swift:2101`) defers its only write,
    `surfaceView.cellSize = ..`, with `DispatchQueue.main.async` (`:2114`), so the `@Published`
    `cellSize` and its `onChange` observer run after the callback has returned.
  - GTK `initial_size` (`application.rs` `action::initial_size`) sets the surface's `default-size`
    GObject property. Nothing connects to `notify::default-size` (the only `connect_notify_local`
    calls in `gtk/` watch `config`, `scale`, `label`, `focused`, `mapped`, `bell-ringing`, `zoom` and
    the key-state overlay's own properties); the window reads the property when it is presented
    (`window.rs:1387`, `application.rs:4520`).
  - So the protector is never violated and no raw-self rewrite is needed; `presentSurface`, the other
    case, was already made raw-self by P7-W6 T3.

## N452 — effect callbacks that re-enter the Terminal C API reach stream/handler bytes under the protected `&mut Stream`/`&mut Handler`

- **id**: N452. P7-W2 T6 (implementer concern 4; task review, controller ruling 51). **severity**: medium
  (aliasing-model UB in the port only; no observed miscompile). **kind**: port soundness (Stacked
  Borrows). **Not upstream**: Zig's `*Stream`/`*Handler` carry no protector, so the reference is sound.
  **state**: CLOSED (fixed, P7-W2 T9).
- **state history**: opened OPEN at P7-W2 T6, owned by the follow-up task P7-W2 T9 (a Phase-7 exit
  item), which closed it.
- **shape**: `rustty_terminal_vt_write` runs `next_slice` with a protected `&mut Stream` (and the
  handler's `&mut self` below it) for the whole call, and effect callbacks (bell, title, clipboard,
  render_hold, …) run synchronously inside it. The reference headers
  (`include/ghostty/vt/terminal.h:78-81`, `:2212-2215` at 12752b2ac; Rustty's header says the same)
  forbid only `vt_write`/`vt_write_until_ground` on the same terminal from a callback. They do NOT
  forbid:
  - `rustty_terminal_resize` (`crates/rustty-vt-c/src/terminal.rs:3867-3879`);
  - `rustty_terminal_set` arms that write handler fields (`CLIPBOARD_WRITE`/`READ`,
    `UNKNOWN_SEQUENCE`, `TITLE_REPORT`, `TERMINFO_NAME`, `UNKNOWN_MAX_BYTES`,
    `CLIPBOARD_WRITE_MAX_BYTES`; `terminal.rs:2888-3546`);
  - `rustty_terminal_get` of `SEMANTIC_FAILURE`, `VT_GROUND`, `KITTY_CLIPBOARD_WRITE_MAX_BYTES`
    (`:4242`, `:4284`, `:4313`);
  - the continuation APIs.

  Each reaches stream or handler bytes through the wrapper pointer while the outer protected `&mut`
  is live: UB under Stacked Borrows, well-defined in Zig. `rustty_render_state_update` from a
  callback is sound (it touches only the separately allocated terminal; proven by
  `render_hold_callback_reenters_render_state_update`, P7-W2 T6).
- **fix direction**: build an isolated harness under `.port/phase-2/mirihar-*` that shows the UB, then
  make effect dispatch raw-self: `next_slice`, dispatch and effect calls over `*mut Stream` /
  `*mut Handler`, with narrow reborrows that end before each effect call (memories
  `rust-mut-args-strongly-protected`, `shaper-hook-sb-clean-interleave`). LIFETIMES rows first. The
  harness must be UB before and clean after, under SB and TB.
- **fix** (P7-W2 T9): raw-self stream drivers, plus the trampolines' own handler pointer for handler
  bytes. No behaviour change.
  - rustty-vt `Stream`: `next_slice_raw`, `next_slice_until_ground_raw`, `next_raw` and every private
    driver that reaches a handler action (`next_slice_untracked`, `next_slice_capped`,
    `next_slice_until_ground_untracked`, `consume_all_escapes`, `consume_until_ground`,
    `consume_apc_string`, `csi_entry_byte`, `consume_csi_params`, `next_untracked`, `next_utf8`,
    `handle_codepoint`, `next_non_utf8`, `csi_dispatch_final`) are `unsafe fn(this: *mut Self, ..)`,
    the reference's `self: *Self`, with narrow place accesses. The pure dispatchers (`print`,
    `execute`, `csi_dispatch`, `esc_dispatch`, `configure_charset`, like `osc_dispatch` already) take
    `handler: &mut H` only. Across a handler action only the handler bytes are protected.
    `ground_raw` and `write_continuation_raw` read by place. The `&mut self`/`&self` methods of the
    same names forward to the raw bodies for Rust callers.
  - The `stream::Handler` trait keeps `&mut self`, so the handler bytes stay protected during an
    action. rustty-vt-c `TerminalWrapper.live_handler` (port-only field): each of the 15 effect
    trampolines derives ONE raw pointer from its `&mut Handler` argument and publishes it for exactly
    the C callback's duration (`LiveHandler` guard: save, set, restore on drop, so nested effects
    nest). `handler_ptr(w)` gives every C API that touches handler bytes that pointer when set, else
    `stream.handler` by place: the `set_typed` handler arms, `get` of `VT_PROCESSING_ERROR` and
    `CLIPBOARD_WRITE_MAX_BYTES`, `resize`, C `reset`'s render_hold, and `rustty_terminal_paste`.
  - The C API reaches stream state by place only: `vt_write` / `vt_write_until_ground` /
    `restore_continuation` through the raw entries, `get(VT_GROUND)`, `setContinuationMaxBytes` and
    `continuationAllocIo` through `ground_raw`, `continuation_write_io` through
    `write_continuation_raw`.
- **proof**: `.port/phase-2/mirihar-n452` (16 seeds, SB+TB). The shipped shape is UB for every API
  class (`n452_shipped_{set,get_ground,continuation}` under SB and TB, `_resize` under TB,
  `_get_handler` under SB); each fix part alone stays UB (`n452_raw_stream_only_set`,
  `n452_stash_only_get_ground`, `n452_raw_stream_whole_ground`); `n452_fixed_*` and
  `n452_fixed_all` are CLEAN under both. The real code: vt-c Miri targets
  `reentrant_callback_resize`, `reentrant_callback_set_handler_option`,
  `reentrant_callback_get_vt_ground` and `reentrant_callback_continuation`, CLEAN under SB and TB,
  with negative controls (T9 report). LIFETIMES rows `src/terminal/stream.zig Stream (self: *Self)`
  and `src/terminal/c/terminal.zig TerminalWrapper live_handler`.
- **residual**: the terminal (`*mut Terminal`, a separate allocation) is outside this finding; the
  effect call sites drop their `term()` borrows before the effect (T6 discipline, re-checked at
  every site in the T9 report). Found by the new set-option test: **N451**.

## N451 — `rustty_terminal_free` moves out a wrapper whose `terminfo_name` borrows the freed wrapper storage

- **id**: N451. P7-W2 T9 (found by `reentrant_callback_set_handler_option` under Miri). **severity**:
  low (aliasing/validity UB in the port only; no observed miscompile). **kind**: port soundness
  (reference validity). **Not upstream**: the reference's `terminfo_name` is a Zig slice, which may
  dangle. **state**: CLOSED (fixed, P7-W2 T9).
- **shape**: `set(.terminfo_name)` publishes `handler.terminfo_name: Option<&[u8]>` into the
  wrapper's own `terminfo_name_buf`. `free` does `ptr::read(terminal_)`, returns the storage through
  `destroy_raw`, then drops the moved value, whose `&[u8]` now points into freed memory. Miri (SB):
  "constructing invalid value of type TerminalWrapper: at .stream.handler.terminfo_name.<Some>.0,
  encountered a dangling reference (use-after-free)". Every free of a terminal with a terminfo name
  set hit it; the existing terminfo test clears the name before `free`, so it never did.
- **fix**: `free` clears `terminfo_name` by place before the move (nothing reads it again). Proven by
  `reentrant_callback_set_handler_option`, which frees with the name set, CLEAN under SB and TB.

## N453 — the overscan port's two reference defects: a stale `rowDataRange()` after a failed update, and `u16` style-run rows (UD369, UD370)

- **id**: N453. Not a ledger row: found by task P7-W3 T1 while porting `ca4f719f7` (RenderState
  overscan). **severity**: low. **kind**: reference defects (crash; UB in ReleaseFast), reachable by
  an embedder only.
- **what**: (1) `beginUpdate` writes `overscan` only after the fallible row loop, so after a failed
  update `rowDataRange()` mixes the previous update's `overscan` with the new request, rows and
  `row_data` length: it ends past `row_data` or underflows, and the C row iterator slices with it
  (UD369). (2) `StyleRun.y` stays `u16` while it now holds a `row_data` index up to three `u16`
  counts (UD370). Both MEASURED as panics at 12752b2ac (`.port/phase-7/probes/p7w3t1_overscan_probe.zig`).
- **fix / proof**: `crates/rustty-vt/src/render.rs`: `begin_update` resets `overscan` on failure,
  `row_data_range`/`viewport_range` clamp to `row_data.len()`, `StyleRun.y` is `u32`. Tests
  `row_data_range_stays_in_bounds_after_a_failed_overscan_update` and
  `style_runs_address_rows_past_u16_with_large_overscan`, each with a negative control recorded in
  the UD entry.
- **state**: CLOSED (task P7-W3 T1).

## N454 — the C paste reader can re-enter the terminal while the paste holds protected handler and terminal borrows

- **id**: N454. P7-W2 T9 task review (finding 1; controller ruling 54). **severity**: medium
  (aliasing-model UB in the port only). **kind**: port soundness (Stacked Borrows). **Not upstream**:
  Zig pointers carry no protector. **state**: CLOSED (fixed, P7-W2 T10).
- **state history**: opened OPEN at the P7-W2 T9 task review, owned by the follow-up task P7-W2 T10 (a
  Phase-7 exit item), which closed it.
- **shape**: `paste_pkg::paste` (`crates/rustty-vt/src/stream_terminal.rs:683-711`, reached from
  `crates/rustty-vt-c/src/paste.rs:227`) holds `Context { terminal: &Terminal, kitty_clipboard: &mut
  grants }` under a protected `&mut Handler`, and pulls data synchronously through the embedder's
  `RusttyMimeReader`. `io.h` (as the reference's) does not forbid the reader from re-entering the
  terminal. The reader is not an effect trampoline, so the T9 `live_handler` is null, and a reader
  that calls `set(TITLE_REPORT)` or `resize` writes protected handler or terminal bytes.
- **fix direction**: either publish the paste's handler pointer around the reader (as the effect
  trampolines do) and make `Context.terminal` a raw pointer, or rule that reader re-entry is out of
  contract and say so in the header, matching the reference's wording. Prove it on the `mirihar-n452`
  harness (a reader arm) and with a vt-c test whose reader re-enters `set(TITLE_REPORT)`.
- **contract ruling** (P7-W2 T10): re-entry is IN contract, so it must be sound. At 12752b2ac the
  reference's `include/ghostty/vt/io.h` (`GhosttyMimeReaderFn`: "invoked synchronously on the calling
  thread") and `paste.h` (`GhosttyPaste::reader`, `ghostty_terminal_paste`) put no restriction on what
  the reader may call. The only no-reentrancy wording in `terminal.h` (`:78-81`, `:2212-2215`) covers
  effect callbacks calling `vt_write`/`vt_write_until_ground`, and the continuation writer (`:2290-2291`)
  forbids terminal APIs only for itself. So a paste reader may call `set`, `resize`, `reset`, even
  `vt_write`. No header change (the headers stay token renames of the reference).
- **fix** (P7-W2 T10): no behaviour change; same calls, same order.
  - rustty-vt `stream_terminal::Handler::paste_raw(this: *mut Self, req)`: the reference's
    `self: *Handler`, with place accesses; the queued pty chunks go out through a narrow `&mut *this`
    per `write_pty` call after the read. The safe `paste(&mut self)` forwards for Rust callers.
  - rustty-vt `paste::Context.terminal: *const Terminal` and `KittyClipboard.grants: *mut Grants` (the
    reference's `*const Terminal` / `*Grants`; `KittyClipboard` loses its lifetime). `paste::paste` is
    `unsafe fn` with the pointer contract; mode reads are narrow reborrows that end before the read,
    and the grants are dereferenced only on the paste-event path (no reader there). rustty-core
    `completeClipboardPasteEvent` passes `addr_of!`/`addr_of_mut!` under its existing mutex.
  - rustty-vt-c `terminal_paste` calls `Handler::paste_raw` over `handler_ptr(w)` and publishes that
    pointer in `live_handler` (`LiveHandler`) for the call, as the effect trampolines do.
  - LIFETIMES rows first: `src/terminal/paste.zig Context terminal`, `Context.KittyClipboard grants`,
    `src/terminal/stream_terminal.zig Handler (self: *Handler) in paste`.
- **proof**: `.port/phase-2/mirihar-n452` reader arm (16 seeds, SB+TB). `n454_shipped_{set,resize,reset,
  vt_write}` UB under both; each fix part alone stays UB under both (`n454_raw_ctx_only_set`: raw
  Context, `&mut self` paste; `n454_raw_self_ref_terminal_resize`: raw-self, `&Terminal` Context;
  `n454_raw_self_ref_grants_reset`: raw-self, `&mut Grants` Context); `n454_fixed_{set,resize,reset,
  vt_write}` and `n454_fixed_all` CLEAN under both. Real code: vt-c Miri target
  `terminal::tests::reentrant_callback_paste_reader` (a reader that re-enters `set(TITLE_REPORT)`,
  `resize` with mode 2048 (nested `write_pty`), a nested `vt_write` (title report + BEL), then RIS
  through `vt_write` (clears the grants) and C `reset`), CLEAN under SB and TB; negative controls
  (reverted): the `&mut self` paste without publication is UB at the `set(TITLE_REPORT)` write (SB and
  TB); a protected `&Terminal` across the paste is UB at the resize (SB and TB); a protected
  `&mut Grants` across the paste is UB (SB at the reader's `resize` retag, TB at the RIS grants deinit).
  Also W2.T9 review minor 3: `reentrant_callback_resize`'s nested `write_pty` callback now re-enters
  `set(TITLE_REPORT)` (nested `live_handler` publication on real code).

## N455 — two vt-c tests have test-side Tree Borrows UB

- **id**: N455. P7-W2 T9 (implementer concerns 2 and 3; task review; controller ruling 54). **severity**:
  low (test code only; the production paths are lifetime-tied or never dereference the userdata).
  **state**: CLOSED by P7-W12 T0 (`16dfbab68`).
- **tests**:
  - `paste::tests::terminal_paste_text_and_unsafe`: a test-only helper stores `self as *mut` as the
    userdata and dereferences it while a `&mut` is live (TB only).
  - `snapshot::codec::tests::complete_snapshot_restores_a_canonical_stream_continuation`: the test builds
    `Handler::init(&mut source_terminal as *mut _)` by hand. The production `Terminal::vt_handler(&mut
    self) -> Handler<'_>` is lifetime-tied, so the compiler rejects the same interleave (TB only).
- **fix direction**: derive every pointer the test later uses from one raw root, then re-run both tests
  under Miri SB and TB.
- **closed (P7-W12 T0, `16dfbab68`)**:
  - the TB mechanism, from Miri: the pointer the callee writes through (the paste reader's userdata;
    the stream handler's terminal pointer) was minted from a `&mut` reborrow, so the test's own read
    through the OWNER in between (`contents.reads[0]`; `encode(&source_terminal)`, `plain_string`) was
    a foreign read that froze that tag, and the next write through it (`(*this).reads[index] += 1` at
    `paste.rs:776`; `self.previous_char = ..` at `terminal.rs:2022`) was a write through a Frozen tag.
  - fix: each object is reached only through one `&raw mut` root, which is also the pointer the callee
    writes through; the test reads and writes (`reads`, `fail`, `plain_string`, the active screens,
    `deinit`) go through that root. `Contents::request_raw(*mut Contents)` builds the request from the
    raw root (MIME list by `addr_of!`); `request(&mut self)` delegates to it. The paste test now reuses
    `req` after `fail = true`, as the reference does, instead of re-minting it.
  - Miri: `cargo +nightly miri test -p rustty-vt-c --lib -- --exact
    paste::tests::terminal_paste_text_and_unsafe` and `cargo +nightly miri test -p rustty-vt --lib --
    --exact snapshot::codec::tests::complete_snapshot_restores_a_canonical_stream_continuation`, each
    plain (SB) and with `MIRIFLAGS=-Zmiri-tree-borrows`: 1 passed, no UB, all four runs. The pre-fix
    tests under TB: "write access through <tag> ... is forbidden", state Frozen (paste) and a child of
    the conflicting tag (codec).

## N458 — a port-only string-map test dropped its `Screen` without `deinit`, tripping the W1.T5 node-pool assert

- **id**: N458. P7-W1 T8 (controller ruling 56). **severity**: none in production (test only).
  **state**: CLOSED by P7-W1 T8 (`29a4030f5`).
- **finding**: `string_map_string_is_unsentineled_and_searchable` (rustty-vt, only built with
  `--features oniguruma`) never called `deinit` on its `Screen`, `StringMap` or regex, where the
  reference tests `defer` all three. Since P7-W1 T5, `UntouchedPool::drop` asserts that every node
  went back to the pool, so the dropped `Screen` aborted the test.
- **audit**: `StringMap`, `SearchIterator` and `Match` hold no `Screen` or `PageList`. The
  production owners' error paths (`screen_set`, snapshot decode and codec, the tmux viewer pane sync,
  termio init) never drop a live `PageList`.
- **fix**: the test now deinits the three objects, as the reference does.

## N461 — the UD351 OSC 7 control test assumed a unix host

- **id**: N461. P7-W12 T1b (carried from W7.T5d). **severity**: none in production (test only).
  **state**: CLOSED by P7-W12 T1b (`f739cd6d9`).
- **finding**: `ud351_osc7_percent_encoded_local_host_still_sets_the_pwd` failed on
  `x86_64-pc-windows-msvc` (`left: Some("/tmp")`, `right: None`), already at `df350e613`. On Windows
  `reportPwd` logs "reportPwd unimplemented on windows" and returns before it parses the URL
  (`stream_handler.zig:1486-1489`, ported as-is), so no OSC 7 sets the pwd there. Not a UD: the
  Windows arm is the reference's declared behaviour and is ported unchanged.
- **fix**: the control expects `None` on Windows, with a PORT NOTE that it pins the early return and
  that the other `ud351_*` rejections hold on Windows only through it.
- **evidence**: rwin, `cargo test -p rustty-termio --lib --target x86_64-pc-windows-msvc ud351`:
  1 failed at `5da45a868`, 6 passed after; the full `-p rustty-termio` run is 74 passed, 0 failed,
  14 ignored.

## N462 — the DMA-BUF strict-mode pivots were dead code on Windows

- **id**: N462. P7-W12 T1b. **severity**: low (CI Windows clippy red). **state**: CLOSED by
  P7-W12 T1b (`d857cb820`).
- **finding**: `cargo clippy --workspace --all-targets -- -D warnings` for `x86_64-pc-windows-msvc`
  failed with `dead_code` on `test_support::dmabuf_required` and `require_dmabuf_or_skip`, whose only
  callers are `#[cfg(unix)]` DMA-BUF KATs (`opengl_impl.rs`, `target.rs`, P7-W7 T1).
- **fix**: both are `#[cfg(unix)]`; the env-independent `require_dmabuf_or_skip_impl` and its unit
  tests stay on every target.
- **evidence**: rwin, the Windows clippy failed with the two errors at `5da45a868` and is clean after.

## N463 — the iocp `log_warn!` shim left a release-only unused variable

- **id**: N463. P7-W12 T1b. **severity**: low (a release-build warning). **state**: CLOSED by
  P7-W12 T1b (`6bf795191`).
- **finding**: `cargo build -p rustty --release --target x86_64-pc-windows-msvc` warned
  `unused variable: err` at `rustty-xev/src/iocp.rs` (`async_notify`): the shim dropped its arguments
  under `#[cfg(not(debug_assertions))]`, and that log is the binding's only read.
- **fix**: the shim tests `cfg!(debug_assertions)`, so its arguments are type-checked and used in
  every profile; release still formats nothing.
- **evidence**: rwin, the release build has no warning after the fix.

## N464 — `dcimgui-sys/ext.cpp` failed a native aarch64 Windows build with clang 23

- **id**: N464. P7-W12 T1b (carried from W7.T5d). **severity**: low (native ARM64 Windows build
  broken; CI's Windows job is x64 only). **state**: CLOSED by P7-W12 T1b (`cd9a6d29a`).
- **finding**: `ext.cpp` includes `dcimgui.h` inside `namespace cimgui`, so the TU's first
  `<stdint.h>` declares `cimgui::uint64_t` and sets the include guard. The OpenGL3 loader's
  `<windows.h>` then reaches the compiler's `<intrin.h>`. clang 23's includes `<arm_acle.h>` on
  aarch64 (`intrin.h:33-34`), which needs `::uint64_t`: "unknown type name 'uint64_t'; did you mean
  'cimgui::uint64_t'?". Port-only, not a UD: the reference's `ext.cpp` is the same, but its pinned
  Zig 0.16 clang (21.1.8) `<intrin.h>` has no `<arm_acle.h>` include, and the reference `ext.cpp`
  compiles for `aarch64-windows-gnu` with that Zig. Rustty compiles with the host's clang-cl
  (`prefer_clang_cl_over_msvc`, N353), so it meets the newer header.
- **fix**: `ext.cpp` includes the five C headers `dcimgui.h` pulls in at global scope before the
  namespace, making the in-namespace includes no-ops.
- **evidence**: rwin, `cargo build -p dcimgui-sys --target aarch64-pc-windows-msvc` failed at
  `5da45a868` and passes after; the native ARM64 `cargo build --workspace --all-targets` and clippy
  `-D warnings` (the CI excludes) then pass. macOS and Linux rebuild `dcimgui-sys` clean.
- **resync note** (fix round 1): `ext.cpp` mirrors the reference's `pkg/dcimgui/ext.cpp`; a resync
  that copies it from upstream must re-apply the RUSTTY-ONLY global include block, or the native
  aarch64 Windows build breaks again.

## N465 — `Surface::close(&mut self)` holds a protected `&mut Surface` across the runtime's close callback, which re-enters the surface

- **id**: N465. P7-W12 T1 (the Phase-7 final review, controller ruling 61). **severity**: medium
  (aliasing-model UB on every GTK close, and on a macOS close while a clipboard confirmation is
  pending on the surface; no observed miscompile). **kind**: port soundness (Stacked and Tree Borrows;
  a function-argument protector). **Not upstream**: Zig's `*Surface` and `*const apprt.Surface` carry no
  protector, so the reference is sound; no UD. **state**: CLOSED (fixed, P7-W12 T1).
- **shape**: `rustty_core::Surface::close(&mut self)` ran `self.rt_surface.close(self.needsConfirmQuit())`
  with `self` protected for the whole runtime callback. It is reached from `childExited(&mut self)` and
  `handleMessageArms(&mut self)` (`.close`, `.child_exited`), `keyCallback`'s child-exited close,
  `performBindingAction`'s `.close_surface`, `App::closeSurface`, and `rustty_surface_request_close`'s
  fallback. The seam's `fn close(&self, process_alive)` added a second protector: the embedded
  `Surface` holds `core_surface` inline, so its `&self` covered the core bytes for the whole callback.
- **host trace** (synchronous re-entry):
  - macOS: `close_surface_cb` → `App.closeSurface` posts `rusttyCloseSurface` (`Rustty.App.swift:291`).
    `NotificationCenter` delivers synchronously to `BaseTerminalController.rusttyDidCloseSurface`
    (`:658`) → `closeSurface(node, withConfirmation: process_alive)` (`:469`). With
    `withConfirmation == false` it runs `removeSurfaceNode` → `replaceSurfaceTree` → the `surfaceTree`
    didSet (`:49-53`) → `surfaceTreeDidChange` (`:336`) → `cancelPendingClipboardConfirmation` (`:1660`)
    → `pendingClipboardConfirmation = nil` → its didSet (`SurfaceView_AppKit.swift:113`,
    `pendingClipboardConfirmationDidChange` `:1910`) → `previous.cancel(from:)` → the completion
    (`Rustty.App.swift:411-423`) → `rustty_surface_deny_clipboard_request` → embedded
    `denyClipboardRequest(&mut self)` → core `denyClipboardRequest`: a write. Only when a clipboard
    confirmation is pending on that surface (an unfocused surface's OSC 52 read keeps it pending). The
    focus move is `DispatchQueue.main.async` (`replaceSurfaceTree`), and `confirmClose` is an alert
    (asynchronous). The surface is not freed synchronously (N442's audit: the notification object,
    the tree node and the undo stack keep the view alive).
  - GTK: `close` emits `close-request`; `SplitTree.surface_close_request`
    (`crates/rustty-apprt/src/gtk/class/split_tree.rs:865`) takes `&mut *core` for
    `needsConfirmQuit()` on every close, then `close_confirmation_close` replaces the tree. The widget
    is not finalized during the emission (N442's audit).
- **fix**: raw-self, N442's shape.
  - Core `close(self_: *mut Surface)` and `childExited(self_: *mut Surface, ..)` are unsafe associated
    fns. `handleMessage` dispatches `.close` and `.child_exited` raw (`handleMessageArms` marks them
    unreachable). `close` ends `needsConfirmQuit`'s reborrow and reads `rt_surface` by place before the
    callback; `childExited`'s unprotected local reborrow is dead before `close`.
  - The seam is `unsafe fn close(self_: *const Self, process_alive)`. The embedded impl reads `app` and
    `userdata` by place; the GTK impl's `&Self` ends with `gobj()`, before the emission.
  - Callers: `keyCallback` and `performBindingAction` call `Surface::close(self_)` (their `this` is dead
    after), `App::closeSurface` passes its surface pointer, and `rustty_surface_request_close` passes
    `&raw mut (*ptr).core_surface`.
- **proof**: `.port/phase-2/mirihar-n465` (16 seeds each). `n465_shipped_macos_child_exited` and
  `n465_shipped_gtk_child_exited` are UB under SB and TB. The negative `n465_core_only_macos_child_exited`
  (core raw-self, seam still `&self`) is UB under both, so both halves are needed. `n465_fixed_*`
  (`.close`, `.child_exited` and the C fallback on macOS; `.close` and `.child_exited` on GTK) is CLEAN
  under both. The negative row is macOS-only: "both halves are needed" holds for the embedded runtime,
  whose surface holds the core inline. The GTK runtime surface is a separate allocation, so there the
  core half is what matters, and no core-only GTK row was run. LIFETIMES row
  `src/Surface.zig Surface [self across rt_surface.close]`.
- **fix round (ruling 62)**: `childExited`'s two `show_child_exited` callbacks now run by place
  (`Target::surface_ptr(self_)`), and `this` is re-taken after each, per N442's convention.
- **test**: `crates/rustty-apprt-c/tests/macos_window_host.rs::close_callback_reenters_the_closing_surface`
  makes a real pending clipboard request (a `paste_from_clipboard` whose read callback starts it and
  keeps the state), then `cb_close_surface` calls `rustty_surface_deny_clipboard_request` on the
  closing surface, as the macOS host's close path does. Miri cannot see across the FFI.
- **host contract**: a host must not free the surface synchronously inside the close callback. The
  surface is touched after the callback returns, as in the reference: `keyCallback`'s `InspEvGuard`
  drop re-reads it, and a chained `close_surface > X` binding runs its next action on it.

## N477 — GTK `--shell-integration=` with an empty value did not reset the override

- **id**: N477. P7-W12 T3 (W12.T2 re-audit, ruling 63). **severity**: low (a GTK tab-override
  argument). **state**: CLOSED by P7-W12 T3.
- **finding**: `Overrides.parse` hands the trimmed value to `cli.args.parseIntoField`, which first
  treats an empty value as "reset to the field default" (`src/cli/args.zig:326-339`); the anonymous
  struct's `@"shell-integration"` defaults to `null`. The port called `string_to_enum` directly, so
  an empty or all-whitespace `--shell-integration=` warned "unable to parse" and kept an earlier
  value.
- **fix**: `crates/rustty-apprt/src/gtk/class/overrides.rs` resets `parsed_shell_integration` to
  `None` when the trimmed value is empty, with a PORT NOTE.
- **test**: `parse_overrides_trims_ascii_whitespace_only` gains three arms: `bash` then empty ->
  None, `zsh` then ASCII whitespace -> None, empty then `fish` -> Fish (rvm, `--features gtk`).

## N478 — the `cursor_left` reference comment predated `d4f45bee3`

- **id**: N478. P7-W12 T3. **severity**: low (comment fidelity). **state**: CLOSED by P7-W12 T3.
- **finding**: the code already carried `d4f45bee3`'s `if (count == 0) return;` after clearing a
  pending wrap, but the quoted reference block above it was the pre-`d4f45bee3` text.
- **fix**: the quoted block in `crates/rustty-vt/src/terminal.rs` `cursor_left` now matches
  `Terminal.zig` at `12752b2ac`. G3b stays GREEN.

## N479 — the apprt-c C-ABI smoke needed a prior `cargo build`

- **id**: N479. P7-W12 T3 (W12.T2 lane A). **severity**: low (test harness). **state**: CLOSED by
  P7-W12 T3.
- **finding**: `c_abi_smoke_compile_link_run` looked for `librustty_apprt_c.{dylib,so,a}` in
  `target/<profile>/`. Under `cargo test` the library an integration test links against is not a
  cargo root, so cargo builds its cdylib and staticlib into `target/<profile>/deps/` and does not
  uplift them; only `cargo build` does. A plain `cargo test` in a fresh clone therefore panicked
  "missing cdylib", and after a source edit the test could link a stale uplifted copy.
- **fix**: `crates/rustty-apprt-c/tests/smoke.rs` links the `deps/` artifacts of the running build
  (cargo names cdylib/staticlib outputs there without a metadata hash) and asserts the test binary
  runs from `deps/`. No skip: a missing artifact still panics (R14).
- **evidence**: macOS, fresh `target/`, `cargo test -p rustty-apprt-c --test smoke` with no
  `target/debug/librustty_apprt_c*` present: 4/4 pass (the old lookup panics there). Linux: rvm
  fresh clone, same command.

## N480 — G-WINCHK red on macOS and Linux: wuffs-sys ignored `RUSTTY_SYS_CHECK_ONLY`

- **id**: N480. P7-W12 T3 (W12.T2 lanes A2 and C2). **severity**: low (gate lane). **state**:
  CLOSED by P7-W12 T3.
- **finding**: G-WINCHK's step 1 runs `cargo check --target x86_64-pc-windows-msvc` with
  `RUSTTY_SYS_CHECK_ONLY=1`, which highway/simdutf/zlib-sys and rustty-simd honour by skipping
  their `cc` builds. wuffs-sys, a mandatory kitty dependency since P7-W5 T2, did not: its codec TU
  failed on macOS (`'stdlib.h' file not found`, no Windows SDK) and on Linux aarch64 (`cc:
  unrecognized '-m64'`).
- **fix**: `crates-sys/wuffs-sys/build.rs` honours the switch. It skips the three `cc` archives but
  still runs bindgen (the Rust side `include!`s it), with the reference's minimal libc headers on
  the include path and Wuffs' public `WUFFS_CONFIG__AVOID_CPU_ARCH` defined; the latter only
  changes `static inline` bodies, which bindgen never binds, and keeps clang's `<intrin.h>` from
  including the SDK's `<malloc.h>`. The gate is unchanged; its step 2 still compiles wuffs natively
  with the switch unset.
- **evidence**: `bash .port/tools/gates/w2f001_windows_check_gate.sh` PASS on macOS and on rvm.

## N481 — G2 WEAKENED `core::keyToMouseShape` (frozen 13, rust 12)

- **id**: N481. P7-W12 T3 (W12.T2 lane C2). **severity**: low (gate bookkeeping). **state**: CLOSED
  by P7-W12 T3.
- **finding**: no assertion is missing. Upstream `0fb6d2940` ("SurfaceMouse: simplify
  keyToMouseShape") deleted the duplicate "no override restores the application shape (mouse
  tracking)" sub-block, so the live test has 12 `testing.expect`s and the port (P7-W8 T2) has the
  same 12. G2 compares against the phase-0 inventory, frozen at 13.
- **fix**: a row in `.port/audits/upstream-assertion-drift.tsv` (`core keyToMouseShape 13 12
  0fb6d2940`). G2 re-verifies each row against the live oracle, so the bar is re-based to 12, not
  removed.
- **evidence**: G2 GREEN, `DRIFT-OK: core::keyToMouseShape — upstream shrank 13->12 in 0fb6d2940;
  rust=12 >= live bar 12`; `check_assertions.py --self-test` PASS.

## N482 — G-REL red on macOS: xtask's CMake tests rebuilt the release lane's rlibs in place

- **id**: N482. P7-W12 T3 (W12.T2 lane C2). **severity**: low (test harness; the release build
  and every unit test were green). **state**: CLOSED by P7-W12 T3.
- **finding**: `cargo test --release --workspace` failed 17 `--doc` targets on macOS with E0463
  "can't find crate for `rustty_os`" (and `rustty_apprt_api`, `rustty_config`, `rustty_input`,
  `rustty_vt`, `objc2_foundation`). The rlibs existed. Loading `librustty_os-<h>.rlib` by hand gave
  E0460 "found possibly newer version of crate `objc2`": objc2 had been recompiled IN PLACE, same
  file name, new crate hash. The cause is `xtask/tests/cmake_lib_vt.rs`
  (`install_layout_is_versioned_like_the_reference`, `lib_version_string_versions_the_library`):
  their `cmake --build` runs `cargo build -p rustty-vt-c --release` in the workspace `target/`
  with `MACOSX_DEPLOYMENT_TARGET=13.0` (CMakeLists.txt, macOS only) and
  `RUSTTY_LIB_VERSION_STRING`. Cargo tracks both variables (objc2, the `-sys` build scripts,
  rustty-vt, rustty-vt-c) but keeps them out of file names, so that nested build recompiled those
  crates over the outer run's copies after the outer run had built other rlibs against them. The
  doctests run last and trip over the stale references. `cargo test --release --workspace --doc`
  on its own showed `Dirty objc2: the environment variable MACOSX_DEPLOYMENT_TARGET changed`.
  Why it was macOS-only in practice: the Linux lanes had no `cmake` (the tests skip), and per-crate
  runs do not include the xtask tests. `CARGO_PROFILE_RELEASE_STRIP`, `--extern` paths, feature
  unification and dylib naming were ruled out: the extern paths were right and the files present.
- **fix**: `CMakeLists.txt` gains `RUSTTY_CARGO_TARGET_DIR` (default the workspace `target/`,
  passed to cargo as `--target-dir`, and the directory the library is taken from). The two
  building tests configure it to `<target>/cmake-lib-vt-tests`, a persistent directory of their
  own. The gate is unchanged.
- **evidence**: macOS, fresh `target/`, `bash .port/tools/gates/check_release_lane.sh` GREEN after
  the fix; RED with the 17 doctest targets before it (same tree, same command).

## N484 — the bash shell-integration gate ran a `.sh` directly, which Windows cannot execute

- **id**: N484. P7-W12 T4 (W12.T2 lane B2). **severity**: low (test portability; no product
  defect). **state**: CLOSED by P7-W12 T4.
- **finding**: `rustty-extra --test bash_shell_integration_gate`,
  `declare_ax_prompt_command_is_installed_and_reports_status_correctly`, panicked on Windows x64 and
  ARM64 with `%1 is not a valid Win32 application. (os error 193)`: it runs
  `bash_shell_integration_exit_status.sh` through `Command::new(<script>)`, which needs a kernel
  that honours the shebang. The test is Rustty-only; upstream's shell-integration tests
  (`src/termio/shell_integration.zig`) run only in its Linux `test` and `test-macos` jobs, its one
  Windows job (`build-libghostty-vt-windows`) runs `zig build test-lib-vt` alone, and the suite's
  XDG tests return `error.SkipZigTest` on Windows.
- **fix**: the test, its `SKIP_EXIT_CODE` and `bash44_required` are `#[cfg(unix)]`, with an R14
  PORT NOTE in the module header. The `RUSTTY_REQUIRE_BASH44` pivot tests stay unconditional.
  Running it through `bash` on Windows was rejected: `bash.exe` on PATH may be WSL's launcher or
  Git's MSYS bash, neither of which is a host Rustty injects `rustty.bash` into.
- **evidence**: rwin, ARM64 and x64: the gate binary passes 2/2 (the pivot tests; the behavioural
  test is compiled out). macOS: 3/3, the behavioural test included.

## N485 — `rustty-inspector` crashed on Windows ARM64: rustc and clang-cl disagree on a variadic HFA

- **id**: N485. P7-W12 T4 (W12.T2 lane B2). **severity**: medium (any inspector draw that shows
  coloured text crashed on native aarch64-pc-windows-msvc). **state**: CLOSED by P7-W12 T4.
- **finding**: `rustty-inspector --lib` exited `STATUS_ACCESS_VIOLATION` on native ARM64 in
  `widgets::pagelist::ws7_tests::pagelist_widget_draw_path_never_restores`; x64 passed. Under cdb
  (WinDbg 1.2606 arm64) the fault is `ldrsb w8,[x8]` in `ImFormatStringToTempBufferV`
  (imgui.cpp:2330) with `x8 = 0x3ecccccd`, called from `ImGui::TextColoredV` <-
  `ImGui_TextColored` (dcimgui.cpp:1011) <- `widgets::page::treeNode` (page.rs:250). On Windows
  ARM64 every argument of a variadic function, the named ones included, follows the variadic
  convention: a composite is never an HFA and travels in general registers as its bytes.
  clang-cl compiled `ImGui_TextColored(ImVec4 col, const char* fmt, ...)` that way: its prologue
  spills `col` from x0:x1 and takes `fmt` from x2. rustc 1.93.1 set up the call as
  `ldur w0..w3` (the four floats of the `ImVec4` HFA, one per register) and `mov x4,x0` (`fmt`),
  so ImGui read `0.4f`'s bits from x2 as the format pointer. It is a rustc lowering defect for
  aggregates passed to C-variadic functions on this target; scalars agree (LLVM's Win64 vararg
  convention handles them for both front ends), and x64 passes a 16-byte struct by reference under
  both. `ImGui_TextColored` is the only binding in `dcimgui.h` that is variadic and takes an
  aggregate by value. Not an upstream defect: Ghostty does not build the inspector for Windows.
- **fix**: `crates-sys/dcimgui-sys`: bindgen blocklists `ImGui_TextColored`; `ext.cpp` gains the
  RUSTTY-ONLY `ImGui_TextColoredPtr(const cimgui::ImVec4* col, const char* fmt, ...)`, Dear
  Bindings' body with `*col`; `src/lib.rs` declares it. The 22 `rustty-inspector` call sites
  (reference `cimgui.c.ImGui_TextColored(col, fmt, ...)`) pass `&col` to it. Uniform on every
  target, so no call site is cfg-split.
- **tests**: `dcimgui-sys` `no_variadic_binding_takes_an_aggregate_by_value` scans the generated
  bindings and the hand-written `extern` blocks of `src/lib.rs` (following `pub type` aliases, since bindgen names the struct `ImVec4_t`) and fails on
  any variadic with a by-value struct or union parameter. It failed before the fix on macOS
  (`["ImGui_TextColored(col: ImVec4)"]`), so the hazard is caught on every host.
  `variadic_aggregate_scanner_negative_control` is its R5 control.
  `kat_text_colored_ptr_formats_its_arguments` checks the shim lays out exactly
  `CalcTextSize("active-42")` for `("%s-%d", "active", 42)`. The runtime fail-first is the
  existing `pagelist_widget_draw_path_never_restores`.
- **evidence**: rwin native ARM64, before: `rustty-inspector --lib` crash as above (3 of 3 in
  lane B2, plus `--test-threads=1`; reproduced under cdb). After, rwin native ARM64 and x64:
  `rustty-inspector --lib` 21/21 and `dcimgui-sys --lib` 8/8. macOS: the same two binaries
  21/21 and 8/8.
- **toolchain note**: this looks like a rustc ABI-lowering bug on aarch64-pc-windows-msvc: a named
  HFA argument of a C-variadic call is lowered as an HFA (`[4 x float]` in the IR). It has NOT been
  reported upstream; whether to report it is the maintainer's decision. Minimal repro: a clang-cl
  callee `void f(ImVec4 c, const char* fmt, ...)` (ImVec4 = four floats) called from Rust as
  `extern "C" { fn f(c: ImVec4, fmt: *const u8, ...); }`, compiled with
  `--target aarch64-pc-windows-msvc --emit=asm`. Re-tested on rustc 1.99.0-nightly
  (6f72b5dd5 2026-07-22), still wrong: rustc emits `w0..w3` = the floats, `x4` = fmt,
  `w5` = 42, while clang 21 for the same C call emits `x0:x1` = the packed ImVec4, `x2` = fmt,
  `w3` = 42.

## N483 — gates hardcoded the Mac paths of the oracle and the zig cache; Linux was BLOCKED without `/Users/gael` symlinks

- **id**: N483. P7-W12 T5. **severity**: low (gate portability; no product defect).
  **state**: CLOSED by P7-W12 T5.
- **finding**: a dozen gate scripts named `/Users/gael/projects/ghostty` (the oracle) and
  `/Users/gael/.cache/zig/p/...` (the zig package cache) as literals: `check_test_counts.py` (G1b,
  about 30 paths), `ci.sh` (G1b's probe), `check_bench_tests.sh`, `check_lz4_byte_exact.sh`,
  `check_rustty_vt_headers.sh`, `check-vt-struct-layouts.mjs`, `check_comment_coverage.py`,
  `check_constants_parity.py`, `d6_cfg_mapping_audit.py`, `parity_names.env` (`ORACLE_TREE`,
  `ORACLE_HEADER_PATH`), `cvt-parity.sh`, `wasm-parity.mjs`, and the `GHOSTTY_SRC` default of
  `cycle_check.py` / `reach_scc.py`. On the Linux lane (rvm) they were BLOCKED until someone planted
  symlinks at `/Users/gael/projects/ghostty` and `/Users/gael/.cache`. Separately,
  `restore_zig_deps.sh` was stale against the pin: it restored zf 0.10.3 and vaxis 0.5.1, while G1b
  reads zf 0.11.0 and vaxis 0.6.0 (its `--verify-only` therefore failed on a correct cache).
- **design**: one rule, the one `_oracle_root.sh` already states. The oracle is `ghostty` BESIDE THE
  MAIN checkout (the main repository's common git directory, so it works from a worktree), with no
  environment override, so a stray checkout cannot stand in for it. Python gates call
  `_gatelib.oracle_root()`, shell gates `rustty_oracle_root "$ROOT"`, the Node gates and
  `cvt-parity.sh` call the shell twin through `bash -c`, and `ci.sh`'s G1b probe derives its `oracle`
  the same way, so it still names the path `check_test_counts.py` reads. The gates that read the
  oracle's working tree stay pin-checked: G1b now turns a missing, non-repository or off-pin oracle
  into BLOCKED (exit 3) through the new `_gatelib.oracle_pin_problem` (the twin of
  `rustty_oracle_pin_status`). The gates that read `origin/main` on purpose (G-BENCH, G-LZ4, the
  VT-headers arms) keep their documented no-pin rule. The zig cache resolves the way zig does,
  `_gatelib.zig_cache_root()`: `ZIG_GLOBAL_CACHE_DIR`, else `$XDG_CACHE_HOME/zig`, else `~/.cache/zig`
  (zig's default on Linux and macOS). A package dir that is absent is still BLOCKED by G1b's
  missing-reference guard.
- **fix**: the sites above; `parity_names.env` no longer carries the oracle paths (its two consumers,
  `check_symbol_manifest.sh` and `check_rustty_h_abi.sh`, derive them, and the latter now also hashes
  its header with `sha256sum` when `shasum` is absent, where it used to skip the freeze check).
  `restore_zig_deps.sh` restores zf 0.11.0 and vaxis 0.6.0 at the URLs and content hashes the oracle's
  `build.zig.zon` names (tarball sha256 computed from the downloads), keeps the libxev that G1b's xev
  rows read, checks the zf `clib.zig` and the vaxis `Vaxis.zig` / `vxfw/*` files G1b reads, and
  reports the oracle's own `zig-pkg/` libxev (the `xev_file` row). It resolves the cache like zig.
- **tests**: `check_test_counts.py --self-test` gains `self_test_paths`: the cache precedence, and the
  R5 negative controls: a wrong-pin, a missing, a non-checkout oracle and a missing pin each yield a
  refusal. `_oracle_root.sh --self-test` (G-ORACLE-SH-SELF) still passes; its per-file counts for
  `check_rustty_vt_headers.sh` were kept by using one mention of `$GH`. An end-to-end copy of G1b
  beside an absent, and beside a wrong-pin, oracle exits 3 on macOS.
- **evidence**: macOS: G1b (`check_test_counts.py`, its `--self-test`, ci.sh's `run_test_counts`),
  G-BENCH, G-LZ4, `check_symbol_manifest.sh`, `check_rustty_h_abi.sh`, VT-HEADERS (incl.
  `check-vt-struct-layouts.mjs`), `check_constants_parity.py`, `d6_cfg_mapping_audit.py`,
  `check_comment_coverage.py`, `_oracle_root.sh --self-test`: GREEN, as before.
  `restore_zig_deps.sh` into an EMPTY `ZIG_GLOBAL_CACHE_DIR` fetched all three packages, matched every
  tarball sha256 and every zig content hash, and ended with G1b GREEN (rc 0). rvm: from a fresh clone
  at `~/projects/p7-w12t5`: G1b, `restore_zig_deps.sh` (rc 0, and `--verify-only` rc 0), G-BENCH,
  G-LZ4, `check_symbol_manifest.sh`, `check_rustty_h_abi.sh`, `check_constants_parity.py`,
  `d6_cfg_mapping_audit.py`, `check_comment_coverage.py`, `cycle_check.py`, `reach_scc.py`,
  `_oracle_root.sh --self-test`: PASS. These rvm runs were first made with the two `/Users/gael`
  convenience symlinks present; the symlinks and the `/Users/gael` directories were then removed and
  the same gates re-run and passed (the symlink-free proof below). No touched script reads
  `/Users/gael` any more (the one remaining literal in `.port/tools` is `perf/ws10_suite.py`'s
  measurement-substrate path, a Mac-only perf tool, not a gate).
- **symlink-free proof**: rvm, from `~/projects/p7-w12t5`, with `/Users/gael` removed entirely: G1b
  and its `--self-test`, `restore_zig_deps.sh` (rc 0, `--verify-only` rc 0), G-BENCH, G-LZ4,
  `check_symbol_manifest.sh`, `check_rustty_h_abi.sh`, `check_constants_parity.py`,
  `d6_cfg_mapping_audit.py`, `check_comment_coverage.py`, `cycle_check.py`, `reach_scc.py`,
  `_oracle_root.sh --self-test`: all pass. (Run during N488's lane; moved here in fix round 1.)
- **fix round 1** (P7-W12 T5): G1b's `--self-test` gains `self_test_main_pin_arm`, which runs a copy
  of `check_test_counts.py` end to end beside a missing and beside an off-pin fixture oracle and
  requires exit 3 with the pin check's own reason (the at-pin fixture is its control). NEGATIVE
  CONTROL (macOS): with the `oracle_pin_problem` call in `main()` replaced by `why = None`, both arms
  FAIL and the self-test is RED (the copies still exit 3, but through the missing-reference guard).
  G-ORACLE-SH-SELF enrols the five new resolver users: arm 5 runs `check_symbol_manifest.sh`,
  `check_rustty_h_abi.sh`, `cvt-parity.sh`, `restore_zig_deps.sh` and (under node)
  `wasm-parity.mjs` with `--print-oracle` beside the env and `GIT_DIR` decoys, and arm 10 scans the
  four shell files and the Node file, with a 10s control for each (a plain oracle read added is
  RED). Arm 5 found a real hole: `restore_zig_deps.sh` took `ROOT` from `git rev-parse
  --show-toplevel`, which follows `GIT_DIR` + `GIT_WORK_TREE`, so a decoy moved its oracle; `ROOT` is
  now script-relative. NEGATIVE CONTROLS (macOS): the old `ROOT` line, an env fallback in
  `cvt-parity.sh` or `wasm-parity.mjs`, and a `git rev-parse` oracle in `check_rustty_h_abi.sh`
  each turn G-ORACLE-SH-SELF RED. `restore_zig_deps.sh --verify-only` now names a missing oracle
  `zig-pkg` libxev separately instead of printing "0 corpus/corpora absent". The wording that said
  the oracle paths come from `parity_names.env` (`PARITY_NAMING.md`, `gates/README.md`,
  `check_constants_parity.py`, `check_rustty_h_abi.sh`, `check_symbol_manifest.sh`) now says
  `_oracle_root.sh` derives them.

## N486 — VT-headers gate is macOS-only beyond its paths, and rvm's oracle `origin/main` is stale

- **id**: N486. P7-W12 T5. **severity**: low (Linux gate portability). **state**: OPEN.
- **finding**: found while validating N483 on rvm. `check_rustty_vt_headers.sh` arms (b), (c) and (d)
  read `target/release/librustty_vt_c.dylib` and use macOS `nm -gjU`, and arms (a3) and (a3b) need
  `cbindgen`, which rvm lacks: the gate is RED/BLOCKED on Linux whatever the oracle path is. Also the
  rvm oracle's `origin/main` is `20abdb50a` (the Phase 6 pin), while macOS's is the pin; the gates that
  read `origin/main` on purpose (`check-vt-struct-layouts.mjs`) therefore diverge from the oracle on
  rvm (`RusttyMouseShape`, `RusttyRenderStateOverscan`, `RusttyRenderStateRowId` "absent from the
  oracle registry"). N483's path fix is not the cause: arm (d) did read the oracle's exports on rvm.
- **next**: make the gate's library and `nm` handling Linux-aware (or state it macOS-only), install
  `cbindgen` on rvm, and move rvm's oracle `refs/remotes/origin/main` to the pin.

## N487 — G-NIX failed on Linux: `librustty-vt` built without libclang for the wuffs-sys bindgen script

- **id**: N487. P7-W12 T5 (from the rvm gates lane). **severity**: low (Nix lane; no product defect).
  **state**: CLOSED by P7-W12 T5.
- **finding**: `nix flake check` failed on aarch64-linux with `failed to build attribute
  'checks.aarch64-linux.librustty-vt' ... Unable to find libclang ... set LIBCLANG_PATH`, raised by
  the `wuffs-sys` build script (bindgen), which `rustty-vt-c` pulls in. `nix/librustty-vt.nix` lacked
  `rustPlatform.bindgenHook`; `nix/package.nix` had it, and its comment wrongly said librustty-vt needs
  none. Port-only: the reference's `nix/libghostty-vt.nix` has no bindgen (zig builds its C
  dependencies), so it has no equivalent.
- **fix**: `nix/librustty-vt.nix` takes `rustPlatform` and lists `rustPlatform.bindgenHook` in
  `nativeBuildInputs`, as `package.nix` does, with a PORTS note; the stale `package.nix` comment is
  corrected.
- **evidence**: rvm (aarch64-linux): before, `nix flake check` failed on `checks.aarch64-linux.librustty-vt`
  with the libclang error; with the hook the bindgen step passes. The derivation then exposed the three
  defects recorded as N488, and G-NIX is GREEN with all of them fixed.

## N488 — the Nix lane had three more defects behind N487: absolute libdir, stale library names, dev-shell-bound formatter check

- **id**: N488. P7-W12 T5. **severity**: low (Nix lane; one CMake install defect). **state**: CLOSED
  by P7-W12 T5.
- **finding**: with N487 fixed, G-NIX failed in three further places, each hidden by the one before.
  1. `CMakeLists.txt` built the soname symlink directory as `DESTDIR + CMAKE_INSTALL_PREFIX + "/" +
     CMAKE_INSTALL_LIBDIR`. nixpkgs passes an ABSOLUTE `CMAKE_INSTALL_LIBDIR` for split outputs, so
     the path nested (`/nix/store/x//nix/store/x/lib/...`) and `cmake --install` failed.
  2. `nix/librustty-vt.nix` still used the old artifact names (`librustty_vt_c.a`, `-lrustty_vt_c`);
     the CMake install ships `librustty-vt.a` and `librustty-vt.so*`, so `postInstall`, the
     sanity-check and the pkg-config and link checks could not match.
  3. G-NIX step 3 ran alejandra through `nix develop --command`, which realises the whole dev shell;
     nixpkgs' `python3.14-wasmtime` fails its own tests on this host, so a formatting check went
     RED for a reason unrelated to formatting.
- **fix**: (1) an absolute libdir is used as is (DESTDIR prepended), a relative one keeps the
  prefix-at-install-time form. (2) `nix/librustty-vt.nix` names follow the install. (3) step 3 runs
  `nix fmt -- --check .`, the flake's own `formatter` (alejandra): same tool and files.
- **evidence**: rvm, from `~/projects/p7-w12t5`: `check_nix.sh` GREEN (7 flake checks, alejandra
  compliant); with a badly formatted `.nix` file added, `nix fmt -- --check .` exits 2 and names it.
  macOS: `check_c_consumer.sh` GREEN after the CMake change (relative-libdir path unchanged).
  The symlink-free re-run of the N483 gates, made during this lane, is recorded under N483.
- **upstream**: item 1 is also a Ghostty defect, recorded as UD375: upstream's `CMakeLists.txt`
  nests the same path, and its `create_symlink` calls have no error check, so the links are
  silently not made. Item 3's dev-shell failure is N489, and the dev-shell build it dropped from
  step 3 is back as G-NIX step 4 (fix round 1).

## N489 — the dev shell cannot be built on aarch64-linux: nixpkgs' `python3.14-wasmtime` fails its check phase

- **id**: N489. P7-W12 T5 fix round 1. **severity**: low (Nix dev shell on aarch64-linux; no product
  defect). **state**: OPEN (blocked on nixpkgs).
- **finding**: `nix build .#devShells.aarch64-linux.default` fails on rvm (nix 2.35.2). The only
  failing derivation is `python3.14-wasmtime-47.0.1`, whose `pytestCheckPhase` stops at collection
  (`ci/cbindgen.py`: `RuntimeError: bindings need an update, run this script`); the python env and
  the `rustty` shell derivation then fail as its dependents. The dev shell pulls it in through
  `python3.withPackages (... python-pkgs.wasmtime ...)` in `nix/devShell.nix`, the same dependency as
  upstream's `nix/devShell.nix:98` at the pin, so upstream's shell has the same exposure. Upstream's
  alejandra job runs `nix develop -c alejandra --check .` (`test.yml:1920`), which realises the
  whole shell. N488 moved G-NIX step 3 to `nix fmt -- --check .`, which stopped the formatter check
  going RED on this failure but also stopped building the shell at all.
- **gate**: `check_nix.sh` step 3 carries a PORT NOTE for the divergence from upstream's command.
  Step 4 builds the shell explicitly (`nix build .#devShells.<system>.default --no-link
  --keep-going`) and classifies the log (`classify_devshell_log`), after stripping ANSI escapes and
  CRs. It consumes only these forms: `error: Cannot build '<drv>'.` with its `Reason:` line, the
  older `error: builder for '<drv>' failed ...`, the propagation lines (`Reason: N dependency
  failed.`, `error: N dependencies of derivation ...`, `error: build of ...`), and the indented
  `> ...` build-log excerpt of the current failing derivation. Every other line that starts with
  `error`, indented or not (a bare `error:` before a `... while ...` trace, `error (ignored): ...`,
  an evaluation, download or hash-mismatch error), is RED. A derivation that failed for its own
  reason is a leaf. Success is GREEN. BLOCKED (exit 3, citing N489) needs all of: every leaf is
  `python3.N-wasmtime-*`; each leaf's reason is `builder failed with exit code N` (not a signal
  such as an OOM kill, and not a timeout); a leaf whose excerpt holds pytest output also carries
  N489's marker `bindings need an update`; and the system is aarch64-linux. Anything else is RED,
  including a failure with no identifiable leaf. Step 2's undefined `red` helper (it would have
  printed "command not found" and carried on) is now defined.
- **tests**: `check_nix.sh --self-test` (G-NIX-SELF, a new `ci.sh` entry; needs no nix) runs 21
  fixture logs, in both nix log formats, including the real rvm log verbatim (BLOCKED), a bare
  `error:` trace, `error (ignored):`, coloured logs (all lines, and one unknown error alone), an
  OOM kill in both formats, a timeout, and pytest output without the marker (each RED).
  NEGATIVE CONTROLS (macOS, fix round 2): each of eleven classifier mutations turns it RED: no ANSI
  strip (the coloured wasmtime-only log and the lone coloured error), the old column-0 `^error: `
  catch-all (the bare and the ignored error), any `Reason:` eligible (OOM, timeout), any old-format
  reason eligible, no marker check, no ineligible-leaf check, no system check (x86_64, darwin),
  dependency failures counted as leaves (the real log), no other-derivation check, no no-leaf
  check, and no old-format parsing.
- **evidence**: rvm (aarch64-linux, gawk 5.3.2), a temporary clone at `5033457bd`: G-NIX-SELF
  GREEN (11/11; also on macOS's BSD awk). Step 4 was run on its own, from a copy of `check_nix.sh`
  with steps 1-3 deleted (its step-4 text diffed identical to the gate's): it built the shell with
  `--keep-going`, the only builder failure was `python3.14-wasmtime-47.0.1`, and it ended
  `G-NIX BLOCKED ... (N489)`, exit 3. Steps 1-2 were not re-run, to keep the Nix work on the VM
  small (they rebuild librustty-vt and the GTK app, and this round does not touch them); step 3,
  `nix fmt -- --check .`, exit 0. The clone was deleted afterwards. Fix round 2: step 4's build was re-run
  on rvm at `6c3299c38` to recapture the log (same single wasmtime builder failure, exit code 2,
  with the marker); the new classifier reads it as BLOCKED (N489), on macOS and on rvm, and it is
  now a self-test fixture. The clone and log were deleted afterwards.
- **next**: re-check when nixpkgs updates `python3Packages.wasmtime`; once the shell builds, step 4
  is GREEN with no change to the gate.

## N490 — `check_symbol_manifest.sh` takes its header from `GHOSTTY_H` and turns a missing header into a WARNING

- **id**: N490. P7-W12 T5 fix round 1 (review of N483). **severity**: low (gate R5). **state**: OPEN
  (recorded only; not fixed in this round).
- **finding**: N483's rule is that the oracle is derived, never set by the environment, and that a
  missing or off-pin oracle is BLOCKED. `check_symbol_manifest.sh` (gate A1) breaks both:
  `GHOSTTY_H="${GHOSTTY_H:-$ORACLE_HEADER_PATH}"` (`:56` after fix round 1, `:53` before) lets the
  environment substitute any header, and when the header is absent `run_check` prints `WARNING:
  pinned header absent ... skipped header<->TSV coverage + count checks` (`:90-91`, was `:87-88`)
  before returning 3. The exit status is BLOCKED, but the text reads as a skip, and the gate never
  checks that the oracle is at the pin (`check_rustty_h_abi.sh` pins the same header's SHA-256).
- **next**: drop the `GHOSTTY_H` override (the `--self-test` can build its fixture another way),
  report the missing header as BLOCKED, and add the header SHA-256 pin or `rustty_oracle_pin_status`,
  each with a negative control.

## N491 — Phase 7 tests missed the G6-only Miri annotation: the Phase 7 briefs ran no Miri

- **id**: N491. P7-W12 T6. **severity**: medium (gate G6 could not finish; no product defect found).
  **state**: CLOSED 2026-09-30 (P7-W12 T2 lane D2): G6c `--self-test` GREEN (both arms; arm 1 took
  1,926 s, mostly libtest filtering under Miri), native re-runs GREEN (`pagelist_teardown_after`
  7/7, `feed_after_complete_discovers_prepended_history_pages` 1/1, `bash_shell_integration_gate`
  3/3). Logs: `.superpowers/p7/laneD2-logs/s1*.log`. REOPENED and CLOSED again 2026-09-30 by
  P7-W12 T6b (see "follow-up" below).
- **finding**: the Phase 7 task briefs ran no Miri, so tests added since the Phase 7 base `284d59d62`
  never got the per-test G6 triage that parity tail C-1 gave earlier waves. Four `rustty-vt` tests
  run past G6's 1,200 s bound (debug assertions on, so `Page::assert_integrity` runs on nearly every
  PageList mutation): `search::screen::tests::feed_after_complete_discovers_prepended_history_pages`
  (W5.T4, `4f144156c`; 4.5+ hours in a G6 run, about 97% in `assert_integrity`, O(rows²) as a
  10-column page fills one newline at a time) and three P7-W1 T5 teardown tests
  (`pagelist_teardown_after_clone_failure_returns_every_node`, `…_clone_from_failure_…`,
  `…_reset_…`). Their siblings carry the WS19 T6 `cfg_attr(all(miri, debug_assertions), ignore)`
  form; these do not. Separately, `rustty-extra`'s
  `bash_shell_integration_gate::declare_ax_prompt_command_is_installed_and_reports_status_correctly`
  (P7-W11) fails under G6 Miri at once: `stat` is forbidden under isolation, and the test spawns a
  shell script, which Miri cannot execute.
- **fix**: the bash-gate test carries a C-1-style isolation exception
  (`cfg_attr(miri, ignore = "MIRI EXCEPTION (rustty-extra, P7-W12 T6): …")`). The four TIMEOUTs each
  passed in G6c conditions under Stacked Borrows and Tree Borrows (10–38 s, no Miri warning) and now
  carry the sibling's exact G6-only form, so they run in G6c. No real Miri finding. Five more
  page-filling Phase 7 tests were measured in G6 (10–113 s) and stay there.
- **evidence**: `.port/phase-2/MIRI_EXCEPTIONS.md`, "P7-W12 T6".
- **follow-up (P7-W12 T6b, 2026-09-30; REOPENED, then CLOSED)**: the V29 re-run (Miri on
  `search::`) stalled for 34 minutes on
  `search::terminal::tests::feed_after_complete_discovers_prepended_snapshot_history` (P7-W5 T4,
  `4f144156c`), which T6's inventory missed. The inventory found test bodies by counting braces per
  line, comments included; in `search/terminal.rs` the Zig-reference comment
  `// while (search.status() != .complete) {` in `starts_feed_required_and_runs_to_complete` has no
  commented `// }` of its own, so the scanner took the rest of the file as that test's body and
  gave every later test in it no row. Method this time: a full native timing of every `rustty-vt`
  lib test (3,313, one process each, debug build), which does not parse source. Exactly four
  unannotated tests take over 0.5 s. The missed Phase 7 test TIMEOUTs at 1,200 s in G6 and in G6c
  under both Stacked and Tree Borrows, so it takes the plain both-lanes form on the 4-criterion basis
  (native 15 s and ASan green; covered in Miri by its `search/screen.rs` sibling in G6c,
  `snapshot::codec::tests::incremental_decode_applies_history_below_live_pty_output` in G6 and
  `starts_feed_required_and_runs_to_complete` in G6). The other three are pre-Phase-7 parity-tail
  S7-6 tests (`b9b72bd23`): `terminal_search_allocation_failure_sweep`,
  `terminal_search_terminal_charged_allocation_failure_sweep` and
  `terminal_search_internals_route_through_the_allocator_handle`. Parity tail C-1's triage measured
  their `rustty-vt-c` counterparts but never these `rustty-vt` ones. They pass in G6 (67 s, 20 s,
  12 s; their corpus writes one page under `cfg!(miri)`) and stay unannotated. No real Miri
  finding. Evidence: `.port/phase-2/MIRI_EXCEPTIONS.md`, "P7-W12 T6b".

## N492 — five `rustty-vt` tests have test-side Tree Borrows UB: a `&mut` reborrow handed to a raw-pointer holder, then touched through the owner

- **id**: N492. P7-W12 T7 (from the W12.T2 lane D V29 run under Tree Borrows). **severity**: low (test
  code only; no production path has the shape). **state**: CLOSED by P7-W12 T7.
- **finding**: V29 under `MIRIFLAGS=-Zmiri-tree-borrows` went RED after 38 s on
  `search::pagelist::tests::simple_search`; Stacked Borrows was clean. Miri aborts the binary at the
  first UB, so every test was then run on its own.
- **mechanism (all five)**: the test minted the pointer that a long-lived holder later writes through
  from a `&mut` reborrow (`PageListSearch::init(.., &mut active.pages, ..)`, `let this: *mut
  PageList = &mut result`, `let p_ptr = &mut p`). The test then touched the owner directly: a read
  (`active.pages.point_from_pin`, `active.selection_string`, `result.assert_integrity()`) froze
  that tag, or a write (`p.grow()`) disabled it. The holder's next write (`untrack_pin` in
  `PageListSearch::deinit`; the node returned to the pool in `PageAllocation::deinit`) was then a
  write through a Frozen or Disabled tag. Zig has no borrow model, so the reference tests have no
  counterpart; nothing to port or report upstream (N-only, not a UD).
- **tests** (all test-side):
  - `search::pagelist::tests::simple_search` (G6);
  - `search::pagelist::tests::feed_with_match_spanning_page_boundary` (G6c);
  - `search::pagelist::tests::feed_with_pruned_page` (G6c; `p.grow()`, the Disabled case);
  - `pagelist::tests::pagelist_page_allocation_stays_detached_until_finalize` (G6);
  - `pagelist::tests::pagelist_page_allocation_rejects_limits_before_modifying_the_destination` (G6).
  - Same shape, not runnable under Miri (both-lanes exception), fixed by construction:
    `search::pagelist::tests::feed_keeps_its_tracked_pin_within_a_shorter_page`.
- **production audit**: clean. `ScreenSearch` hands `PageListSearch` a pointer built with
  `addr_of_mut!((*self.screen).pages)` from the raw `*mut Screen`; `TerminalSearch` builds its
  `ScreenSearch`es from the terminal's raw `screens.all` pointers and reads the active pages
  through the same raw root; the search thread holds a raw `*mut Terminal`. The production
  `allocate_page` callers (`snapshot/history.rs`) use `addr_of_mut!` too. No reborrow tag is ever
  minted for the holder, so later accesses through the root are child accesses, not foreign ones.
- **fix**: the established raw-root idiom (`search/screen.rs`, N455). Each object is reached
  through one raw root that carries no retag (`&raw mut`, or a `list()` closure re-deriving `&raw
  mut (*active_ptr).pages`). The holder gets that pointer, and every later test access goes
  through it. No ignore added, no assertion changed, Zig reference comments kept.
- **evidence** (G6 = `cargo +nightly miri test -p rustty-vt --lib`; G6c = the same with
  `RUSTFLAGS="-C debug-assertions=off" MIRIFLAGS=-Zmiri-disable-isolation`; TB adds
  `-Zmiri-tree-borrows`):
  - enumeration, one test per process, `--exact`, under TB: all 52 G6 `search::` tests (1 UB) and
    all 31 G6c ones (2 UB). The other 7 `search::` tests are ignored in both lanes.
  - each fixed test, in its lane: passes under SB and TB.
  - after rebasing onto `7ca5d9f9a`, the whole `search::` filter in one G6 process: TB 52 passed,
    38 ignored, rc 0 (304 s); SB 52 passed, 38 ignored, rc 0 (143 s). Before the fix, TB aborted
    at `simple_search` after 38 s.
  - native `cargo test -p rustty-vt --lib search::`: 90 passed.
- **`page::` and `pagelist` (V27/V28 scope) under TB, partial**: 383 tests run in G6 after removing
  the `search::` ones. 329 of them ran one per process under TB: 327 ok and 2 UB (the two
  `pagelist_page_allocation_*` tests above, fixed). Not reached, because the host slept: 54 G6
  tests (28 `pagelist::tests`, 26 `snapshot::page::tests`) and the 96 G6-ignored (G6c or both-lanes)
  tests. Lane D's SB runs of V27/V28 cover their SB side. Their TB side is still open.

## N493 — a Phase 7 test helper compared memory byte by byte: intractable under Miri

- **id**: N493. P7-W12 T8. **severity**: low (gate G6 could not finish one test; no product defect).
  **state**: CLOSED 2026-10-01.
- **found**: the lane D V28 run sat on `pagelist_memory_pool_never_touches_idle_page_memory`
  (P7-W1, `32d6c4b51`) for over 30 minutes under G6. The test lays a sentinel over
  `PAGE_PREHEAT * std_size()` bytes and calls `raw_all_equal` over the whole buffer. The helper ran
  `slice.iter().all(|&b| b == value)`, a per-byte interpreter loop under Miri. The page integrity
  check is not involved, so G6c would not have helped.
- **fix**: `raw_all_equal` (`crates/rustty-vt/src/pagelist/tests.rs`) now compares 4,096-byte chunks
  against a pre-filled buffer with slice `==` (one `memcmp` per chunk, which Miri runs natively).
  Same semantics: true iff every byte in `[ptr, ptr+len)` equals `value`; the Zig reference comment
  (`std.mem.allEqual`) stays. The temporary `&[u8]` per chunk is sound: the test holds only raw
  pointers, and each view ends before the next write. New negative test
  `raw_all_equal_detects_a_differing_byte` (middle byte, last byte, partial tail chunk, zero length).
- **measured**: native pass. Miri G6 config (`cargo +nightly miri test -p rustty-vt --lib -- --exact`),
  Stacked Borrows: test 0.83 s (26 s wall with the build). Tree Borrows
  (`-Zmiri-tree-borrows`): test 0.81 s (15 s wall). Both clean, no warning.
- **scan**: `page_tests.rs` compares page memory with slice `==` already. `pagelist/tests.rs` still
  has `from_raw_parts(..).iter().all(|&b| b == 0)` checks on decommitted pool items (about lines
  2718, 3306, 3506, 3566, 4820, 4831); they were not reported slow in the V28 run and are left alone.

## N494 — `rustty-vt` kitty tests reach the Wuffs C FFI under Miri and abort the whole G6 binary

- **id**: N494. P7-W12 T9. **severity**: low (gate G6 aborted the `rustty-vt` lib binary, so every test
  after the first one went unchecked; no product defect). **state**: CLOSED 2026-10-01.
- **found**: the full G6 lane (`check_leaks.sh --all-done`) aborted at
  `kitty::graphics_exec::tests::kitty_graphics_route_through_the_allocator_handle` with
  `can't call foreign function rustty_wuffs_pixel_format__is_direct on OS macos`. Miri aborts the whole
  binary on an unsupported foreign call; it does not fail the one test.
- **origin**: parity tail S7-4 (`9f8a92741`, "kitty graphics, PNG decode and APC routing tests"), a
  rustty-only routing test with no reference counterpart. Its sibling
  `kitty_chunked_loading_routes_through_the_allocator_handle` passes because it only transmits (no
  conversion runs). The routing test's animation frame converts an RGB image to RGBA, and the frame
  composite runs the src-over kernel; both are Wuffs calls.
- **enumeration** (code reading, then targeted G6 runs of `kitty::`, `sys::tests`, and the animation
  tests one filter at a time): the Wuffs entry points in the vt lib are `rgba_from_format` (RGB, gray,
  gray-alpha swizzles), `rgba_src_over` (alpha blend), `zlib::decode` and `png::decode`. Tests that reach
  the first two without an ignore: 21 `kittygfx_animation_*` tests in `graphics_exec.rs` (every a=f / a=c
  step converts the RGB base image) and 3 in `graphics_pixel.rs` (`rgba_conversion`,
  `alpha_blend_source_over_semantics`, `wuffs_and_scalar_src_over_agree`). zlib tests were already excepted.
  The PNG decode tests (`sys::tests::default_decoder_*`, `kitty_graphics_png_transmit_*`) are behind the
  `wuffs` feature, which G6 does not enable. Every other `kitty::` test passes under Miri.
- **fix**: (1) the routing test keeps running in G6 with a `cfg!(miri)` corpus: RGBA payloads (`f=32`)
  and `X=1` (overwrite) on the frame, so no swizzle or blend runs. All routing counts hold except the two
  RGBA conversions, which an RGBA frame over an RGBA image does not make: the Miri expectation is 4 calls
  instead of 6, and those two conversions are asserted natively only. (2) The 24 tests above carry the
  existing `MIRI EXCEPTION` annotation (rows in `MIRI_EXCEPTIONS.md`); their purpose is the Wuffs-backed
  conversion and blend itself, so no input avoids the FFI.
- **measured**: ASan lane (`gates/sanitizers.sh rustty-vt "kitty::graphics_exec kitty::graphics_pixel"
  address`): 78 ran, all pass, no finding. Native: 32 passed. Miri G6 config
  `cargo +nightly miri test -p rustty-vt --lib -- kitty::`: 344 passed, 47 ignored, no abort (509 s).

## N495 — `rustty-cli`'s Mach-O constructor cannot link under AddressSanitizer

- **id**: N495. P7-W14. **severity**: low (every crate that links `rustty-cli` failed to BUILD under
  `-Zsanitizer=address` on macOS, so G-ASAN could not run `rustty`, `rustty-cli` or `rustty-apprt-c`; no
  effect on a normal build). **state**: CLOSED 2026-10-01.
- **found**: G-ASAN's first run (`sanitizers.sh --gate asan`, macOS arm64): `rustty` and `rustty-apprt-c`
  BLOCKED with `ld: initializer pointer has no target in ... librustty_cli-*.rlib`.
- **cause**: `io_file.rs`'s pre-main probe is a `#[used] #[link_section = "__DATA,__mod_init_func"]`
  function-pointer static (W-4b, N365). ASan instruments globals by wrapping each in a larger object with a
  trailing redzone. LLVM exempts globals in the ELF `.init_array`/`.fini_array` sections, but not Mach-O's
  `__mod_init_func`, so the redzone's zero bytes become extra "initializer pointers" and ld64 rejects the
  object. Reproduced on a 10-line crate (with and without the fix). Rust-only: Ghostty has no ASan build and
  registers no constructor this way, so no UD.
- **fix**: `crates/rustty-cli/build.rs` sets `cfg(rustty_asan)` when cargo reports `CARGO_CFG_SANITIZE`
  containing `address` (`cfg(sanitize)` itself is unstable), and the Apple static carries
  `#[cfg_attr(rustty_asan, sanitize(address = "off"))]`, with `#![cfg_attr(rustty_asan, feature(sanitize))]`
  in `lib.rs`. Both only exist in a sanitizer build, which needs nightly anyway; stable builds are unchanged.
- **measured**: G-ASAN after the fix: `rustty` 17 tests, `rustty-cli` 123, `rustty-apprt-c` 52, all green.
  The regression test is `io_file::tests::a_write_to_a_stdout_closed_at_exec_fails_with_ebadf`, which passes
  under G-ASAN only if the constructor both links and runs.

## N496 — first sanitizer-lane runs: test-side defects and lane configuration

- **id**: N496. P7-W14. **severity**: low (tests and gate configuration; no product defect). **state**:
  CLOSED 2026-10-01.
- **found**: the first G-ASAN (macOS) and G-LSAN (rvm, aarch64 Fedora) runs. Each item below was RED or
  BLOCKED for a reason that is not a memory error in product code:
  1. `rustty-vt` `terminal::tests::attrs::terminal_print_slice_charset_matches_scalar_printing` leaked both
     terminals of each of its 4 iterations (LSan: 12,544 bytes in 56 allocations): the port dropped the
     reference's `defer scalar.deinit(alloc); defer batched.deinit(alloc);`. Fixed: the test deinits both.
  2. `rustty-os` `homedir::tests::home_from_shell_returns_the_spawn_error_when_no_pipe_can_be_created`: its
     child lowers `RLIMIT_NOFILE` to 3, and LSan's exit-time scan cannot open `/proc/<pid>/task` with 3
     descriptors ("LeakSanitizer has encountered a fatal error"). Fixed: the child restores the limit
     after the check, before exiting.
  3. `rustty-termio` `command::tests::command_{os,rt}_pre_exec_{1,2}` and
     `exec_failure_in_child_reports_the_errno_tag`: their forked children leave through `posix_exit` (libc
     `exit(3)`, the reference's `posix.exit`), so LSan scans them; a child of a multithreaded process keeps
     only the forking thread, and every block the parent's other test threads hold is reported as leaked
     (4 reports, each the parent's heap, plus the 5 tests failing on the child's exit code 23). Fixed: the
     test module defines LSan's documented `__lsan_is_turned_off` hook, non-zero only in a forked child of
     the test binary (pid recorded by `fork_serial`); the test binary itself is still checked.
  4. `rustty-apprt-c` `tests/smoke.rs` `c_abi_smoke_compile_link_run` (Linux): an instrumented cdylib leaves
     the `__asan_*` runtime to the executable, so the C program failed to link. Fixed: under
     `cfg(rustty_asan)` (new `build.rs`) on Linux the program is linked by clang with
     `-fsanitize=address`. Once it linked, it found N498.
  5. `rustty-vt` `hyperlink::hyperlink_dupe_tests::try_dupe_bytes_reports_allocation_failure` asks for
     `isize::MAX` bytes to test `try_reserve` failure; ASan aborts on a request over its 1 TiB maximum
     ("allocation-size-too-big") where the system malloc returns NULL. Lane fix:
     `allocator_may_return_null=1`. That does turn off three ASan reports (corrected in fix round 1): allocation-size-too-big, calloc-overflow and out-of-memory, each of which becomes a NULL return instead of an abort; memory-error, leak and uninitialized-read detection are unaffected.
  6. `rustty-vt` `tmux::layout::tests::deeply_nested_layout_is_a_syntax_error_beyond_max_depth` parses
     MAX_DEPTH (1000) levels on a default 2 MiB thread: 1.25-1.5 MiB natively (debug; measured with
     `RUST_MIN_STACK`), a stack overflow under ASan, whose redzones grow every frame. Lane fix:
     `RUST_MIN_STACK=8388608` (threads with an explicit stack size are unaffected; unbounded recursion still
     overflows; the native bound stays checked by `cargo test --workspace`).
  7. G-LSAN's GTK apprt lane: ld.bfd died with SIGBUS linking 8 instrumented test binaries at once with full
     debuginfo, after the 10 GB VM stopped answering for about 25 minutes. Lane fix: line-tables-only
     debuginfo (`CARGO_PROFILE_{DEV,TEST}_DEBUG`), which keeps every function, file and line in reports.
  8. The non-vacuity count summed `running N tests` headers, so ignored tests counted as run (`rustty-wasm`,
     whose one native test is `#[ignore]`d, showed "1 test(s) ran"). It now sums `N passed; M failed`.
     Members with no test on a host OS by construction (`gettext-sys` on Linux, `rustty-wasm` everywhere)
     are listed in `NO_TESTS_ON` with the reason; a test appearing there makes the entry stale (RED).
  9. G-MSAN: the classifier read MSan's informational "WARNING: MemorySanitizer failed to allocate" (the
     out-of-memory tests under `allocator_may_return_null=1`) as a finding; it now matches only
     `WARNING: <Sanitizer>: <kind>`. Plain `CFLAGS=-fsanitize=memory` also reached `rustty-cli`'s
     `zlib-sys` build-dependency, whose uninstrumented build script then failed to link; the C/C++ now goes
     through compiler wrappers keyed on `CARGO_CFG_SANITIZE`. Under MSan the C-ABI smoke program cannot
     run (clang 22's MSan runtime does not match rustc's instrumentation: SEGV in the cdylib's first
     constructor), so that one test is ignored under `cfg(rustty_msan)` with the reason; it runs natively
     and under G-ASAN/G-LSAN. The rest of G-MSAN's first-run results are SANITIZER_SUPPRESSIONS.md M1-M5.
- **records**: `.port/phase-2/SANITIZER_SUPPRESSIONS.md` lists the lane options with these reasons.

## N497 — a Miri exception claimed ASan coverage for a test no ASan run executed

- **id**: N497. P7-W14. **severity**: low (a compensating control that did not run; no product defect).
  **state**: CLOSED 2026-10-01.
- **found**: G-ASAN's new cross-check (every `#[cfg_attr(miri, ignore = "...")]` whose reason claims ASan
  coverage must appear as `test ... ok` in the G-ASAN logs) on its first complete run: 116 claims, 115
  verified, and `rustty-vt` `kitty::graphics_image::image_load_png_zlib_compressed_direct` reported
  `ignored`. The test is also `#[cfg_attr(not(feature = "wuffs"), ignore)]`, and no ASan lane ever enabled
  the `wuffs` feature, so "covered natively + by ASan" (MIRI_EXCEPTIONS.md, the zlib C FFI row) was never
  true for it under ASan. It is not run by `cargo test --workspace` either (the feature is off there).
- **fix**: G-ASAN and G-LSAN run a `rustty-vt:wuffs` feature lane. Measured: macOS G-ASAN
  `rustty-vt:wuffs` 3,360 tests, green; the cross-check then verified all 116 claims on macOS. Final G-ASAN (2026-10-02): 117 of 117 (P7-W14 added one), G-LSAN on rvm: 86 verified, 31 macOS-only.
- **fix round 1**: the review held that design §B means every Miri-excepted test, not only those whose
  reason mentions ASan. The check (`san_miri_crosscheck.py`) now covers all 468 tests carrying
  `cfg_attr(miri | all(miri, ..) | any(miri, ..), ignore...)`, matches by module path, attaches an
  annotation only to the `fn` directly after it, and reads the per-OS expected-absent list
  (`san_expected_absent.tsv`; stale rows RED). It found one more test no lane compiled,
  `tmux::control::concurrent_first_notifications_do_not_race_oniguruma_init` (feature `oniguruma`); the
  lane is now `rustty-vt:wuffs,oniguruma`. Measured: macOS G-ASAN 459 ran, 9 expected absent, 0 problems;
  Linux G-LSAN logs 412 ran, 56 expected absent, 0 problems.

## N498 — `rustty_app_free` leaked the app's whole cloned configuration

- **id**: N498. P7-W14. **severity**: low (a leak of one configuration per `rustty_app_free`; the macOS app
  frees its app once, at exit). **state**: CLOSED 2026-10-01.
- **found**: G-LSAN (rvm), once the C-ABI smoke program linked under ASan (N496 item 4): 19 leak reports,
  47,322 bytes in 559 allocations, every stack under `Config::clone_config` from `embedded::App::init`
  (`crates/rustty-apprt/src/embedded.rs:386`) through `rustty_app_new`. `smoke.c` frees the app and its
  own config.
- **cause**: the reference's `App.terminate` calls `self.config.deinit()`, which frees the clone's arena and
  with it every value. The port's `Config::deinit` only clears the arena flag, because each value owns its
  bytes and frees them on drop, and `rustty_app_free` releases the box as `ManuallyDrop<App>` (so the
  already-terminated keymap is not dropped twice), which never drops the config either. Port-only: Ghostty
  frees the arena, so no UD.
- **fix**: `rustty_app_free` (`crates/rustty-apprt-c/src/lib.rs`) drops the config in place after
  `terminate()` and before releasing the box.
- **regression test**: `crates/rustty-apprt-c/tests/smoke.rs` `c_abi_smoke_compile_link_run` under G-LSAN,
  whose C program creates and frees an app; it is RED without the fix.

## N499 — the `screen-clone` benchmark's in-process tests leaked every `clone`-mode copy

- **id**: N499. P7-W14 (fix round 1 reclassified it from a draft UD). **severity**: low (tests only; the
  benchmark CLI behaves as the reference). **state**: CLOSED 2026-10-02.
- **found**: G-LSAN's first run (rvm): `rustty-bench` 720,000 bytes in 6,000 allocations from
  `ScreenClone::step_clone`, under `screen_clone_every_mode_name_parses_and_runs_setup_with_zero_loops`.
- **not a Ghostty defect**: the reference's benchmark CLI allocates with `c_allocator`, no leak checker
  (`src/benchmark/cli.zig:56`), and `ScreenClone.zig:265-266` (also `:295-296`, `:325-326`) leaves the copies
  to process exit on purpose ("we purposely do not free memory because we don't want to benchmark that").
  For a process that exits after the run that is sound. The port's problem is that its unit tests run the
  step inside the long-lived test process, so the copies are real leaks there.
- **fix**: `crates/rustty-bench/src/screen_clone.rs`: `stepClone` puts its 1,000 copies in a fresh
  `Vec::with_capacity(1000)` chunk per step (`clones: Vec<Vec<Screen>>`), so the timed loop never reallocates
  and no earlier copy is ever moved (a first draft reserved on one growing `Vec<Screen>`, which copied every
  earlier `Screen` on growth inside the timed step); `teardown`, which runs after `Benchmark::run` takes its
  end time, and `destroy` free them.
- **regression test**: `screen_clone::tests::screen_clone_clone_mode_frees_its_copies_at_teardown` (two
  steps give two full chunks that never grew; teardown and a full run leave none); G-LSAN's `rustty-bench`
  lane is the leak check.

## N501 — macOS had no leak lane because of a wrong premise: G-LSAN runs there

- **id**: N501 (provisional; P7-W16a, another task may take ids in parallel). **severity**: medium (gate
  coverage: every leak in code that only runs on macOS went unchecked, and one real reference leak, UD376,
  was hidden by it). **state**: CLOSED 2026-10-02.
- **found**: P7-W16a's probe of the BLOCKED sanitizer gates. `sanitizers.sh` reported G-LSAN BLOCKED on
  macOS because "rustc rejects -Zsanitizer=leak on Darwin and Apple's ASan has no leak checker", and
  `check_leaks.sh`, `check_valgrind.sh`, `ci.sh`, `HACKING.md` and PORT_RULES R6 repeated it. Both halves
  are true and neither applies: the STANDALONE `-Zsanitizer=leak` is rejected for aarch64-apple-darwin, and
  APPLE CLANG's ASan runtime aborts on `detect_leaks=1` ("detect_leaks is not supported on this
  platform"), but G-LSAN is ASan with `detect_leaks=1` (design §B), and RUSTC's ASan runtime (its own
  compiler-rt, LLVM 22.1.8) runs the leak checker on Apple Silicon: a probe's deliberate leak is RED
  (64 allocations reported), a clean test GREEN. Evidence and citations: SANITIZER_SUPPRESSIONS.md,
  "Platform support on macOS".
- **fix**:
  1. G-LSAN runs on macOS (`gate_asan_like`: BLOCKED only on a host other than Linux and macOS, which is
     unverified), with its own suppression file `.port/phase-2/lsan-darwin.supp` (S3-S18: libobjc,
     CoreFoundation, SkyLight, CoreText, CoreAnimation, ColorSync, Metal and dyld thread-local state that LSan on Darwin does
     not see as roots; Apple's `leaks` reports 0 leaks for the same binaries, and a CoreText probe with no
     Rustty code reproduces S9/S10 exactly).
  2. `rustty-termio`'s fork-child `__lsan_is_turned_off` hook (N496 item 3) is compiled on macOS too, and
     G-LSAN builds with `-Zexport-executable-symbols` on macOS: the ASan runtime there is a dylib that
     looks the hook up dynamically, and a Rust executable exports nothing by default, so without the flag
     the hook was never called (measured: an always-1 hook did not stop a probe's leak report; the forked
     children of `rustty-termio`'s Command tests reported the parent's heap, 537 allocations).
  3. G-SAN-SELF runs the leak arms on macOS: the deliberate leak RED, a clean control GREEN, the leak
     GREEN under G-ASAN (checker off), and an "opt-out hook reaches the runtime" arm (GREEN; RED in a
     mutation run without `-Zexport-executable-symbols`).
  4. G-MSAN's BLOCKED message on macOS now cites why (compiler-rt has no Darwin MSan; rustc and Apple
     clang reject it) and names the coverage that exists (Miri's uninit checks; G-MSAN on Linux).
- **first findings on macOS**: UD376 (the inspector's Metal backend built without ARC; fixed in
  `crates-sys/dcimgui-sys/build.rs`). The `rustty` crate's `cli_log_facade` test failed only because the
  CLI child's exit status carried LSan's libobjc reports (S3/S4); with `lsan-darwin.supp` the CLI exits 0.
- **result** (macOS arm64, 2026-10-02, `--per-crate-target`): the first full pass (9,397 tests, 57 min)
  was RED and found UD376, N502 and N503, all fixed, plus the system-library reports suppressed as
  S3-S18 (each arbitrated with Apple `leaks`). After the fixes: **G-LSAN on macOS GREEN**, 9,850 tests
  over the workspace and the `rustty-vt:wuffs,oniguruma` lane, 0 findings, 33 min (`rustty-wasm` no
  tests by construction); Miri-exception cross-check 459 ran and passed, 9 expected absent, 0 problems.
  The cross-check's parser now searches result lines instead of anchoring them, since a test's
  unterminated stdout can prefix one (rustty-cli's pager and picker tests). G-SAN-SELF GREEN on macOS
  (52 arms, the leak arms included).
- **fix round 1** (review): S9 narrowed from the whole `libFontRegistry.dylib` to three registry frames,
  S4 anchored, and six Darwin G-SAN-SELF arms keep the patterns honest (a leaked CTFont, a created and a
  copied CTFontDescriptor, a CGFont and an `MTLCopyAllDevices` array must each be RED under the real
  file, a CoreText create-and-release control GREEN). Full Mac pass under heavy load (97 min, 9,658
  tests, cross-check 459 ran / 9 expected absent / 0 problems): every crate GREEN except
  `rustty-font-shaper` (S9's third frame, found by the narrowing; added) and `rustty-apprt-c` (an
  ASan-runtime `fork` deadlock, killed by PID; SANITIZER_SUPPRESSIONS.md "Known limits"); both re-ran
  GREEN (141 and 51 tests). G-SAN-SELF GREEN (58 arms).
- **fix round 2**: `sanitizers.sh` bounds every crate (N506's hang is RED, never a stalled pass);
  S9's third frame became `XTCopyPropertiesForFont`, covering the per-font property query a second
  intermittent `rustty-font-shaper` report showed, with a derived-character-set arm added to G-SAN-SELF.
  Final full Mac pass (2026-10-02, with the bound in place): **G-LSAN GREEN**, 9,850 tests, 0 findings,
  29 min, cross-check 459 ran / 9 expected absent / 0 problems. G-SAN-SELF GREEN (63 arms).
- **still Linux-only**: G-MSAN (no Darwin MSan in compiler-rt) and HWASan (not a gate; Linux task).

## N502 — leaks behind every closed surface, on every platform (only macOS tests reached them): the render thread's and renderer state's Rust-owned heap was never dropped

- **id**: N502 (provisional; P7-W16a). **severity**: low (a few hundred bytes per closed surface;
  port-only). **state**: CLOSED 2026-10-02 (the fix is platform-independent); the Linux test coverage gap
  below stays OPEN.
- **found**: G-LSAN's first macOS run (P7-W16a), `rustty-apprt-c` `tests/macos_window_host.rs`, confirmed
  independently by Apple `leaks --atExit` (`MallocStackLogging=1`) on a native build of the same test
  binary. Two items:
  1. **Test-side**: `app_new_serialized` handed `rustty_app_new` a fresh config and never freed it
     (`App::init` clones it), and one `rustty_app_update_config` call did the same: 14 configs per run,
     about 800 KB with their keybind tables. Fixed: the helper consumes and frees the config, as the
     Swift host's `Config` frees its own after `ghostty_app_new`; the update call frees its config.
  2. **Product**: `Surface::deinit` tore the render thread's block and the renderer-state block down with
     their explicit `deinit`, then freed both allocations WITHOUT drop glue (`ThreadSharedBox::dealloc`,
     N444), so the Rust-owned heap that only `Drop` frees leaked with every closed surface:
     - the xev loop's boxed `thread_pool_completions` queue (`Box<TaskCompletionQueue>`, 280 bytes,
       `crates/rustty-xev/src/kqueue.rs:571`; `Loop::deinit` closes the fds only);
     - the render thread's `DrawMutex` handle (N437), an `Arc` clone of the renderer's, with the boxed
       `pthread_mutex_t`/`pthread_cond_t` std uses on Darwin;
     - the renderer state's D4 `HandoffPark` std `Mutex`/`Condvar`, boxed lazily on Darwin.
     **Platforms (fix round 1 correction):** the teardown is shared, so the leak was not macOS-only. On
     Linux the same glue-less free leaked the `DrawMutex` `Arc` clone and the loop's box (the dynamic
     `Loop`'s epoll/io_uring arm; its N406 `Drop` never ran); only the `HandoffPark` boxes are
     Darwin-specific (Linux std mutexes are futexes). Linux G-LSAN stayed GREEN only because no Linux test
     closes a core `Surface`.
     Measured before the fix: LSan, 14 surfaces → 14 of each; Apple `leaks`, same binary natively (item 1
     fixed): 83 leaks, 9,344 bytes, every stack through `renderer_seam::Thread::init`, `DrawMutex::new`
     or `HandoffPark::gate`.
- **Linux coverage gap (OPEN)**: no test that G-LSAN (or G-VALGRIND) runs on Linux creates and closes a
  core `Surface`. On Linux a surface comes only from the GTK runtime (the embedded platform is macOS/iOS,
  as in Ghostty), and the GTK tests that open one (`crates/rustty-apprt/tests/gtk_*_live.rs`) are
  `#[ignore]`d live tests run through `.port/tools/gtk_live_headless_sway.sh`; no sanitizer lane runs
  them. Closing it needs a lane that runs one of them (e.g. `gtk_window_live`) under ASan with
  `detect_leaks=1` inside the headless-sway wrapper on rvm, with `lsan.supp`'s Mesa entries; not done in
  P7-W16a.
- **not a Ghostty defect**: the reference keeps the queue inline (libxev `backend/kqueue.zig:87`
  `thread_pool_completions: TaskCompletionQueue`), the draw mutex inline (`generic.zig:113`
  `draw_mutex: std.Io.Mutex = .init`), and the state's handoff is atomics only (`renderer/State.zig:40-45`);
  none of them allocates. Port-only, so no UD.
- **fix**: `crates/rustty-core/src/surface/lifecycle.rs` `Surface::deinit` frees both blocks with
  `ThreadSharedBox::drop_and_dealloc` after their teardown (the render thread: `deinit`, then drop;
  the state: `preedit` taken and `mutex` freed as before, then drop), and `initErrdefers` drops the state
  block the same way (it already dropped its read-out render-thread copy after `deinit`). No field's
  `Drop` repeats a `deinit`: rustty-xev has no `Drop` except the dynamic `Loop`'s (N406), which drops only
  the live arm's heap; `Compression`, the completions, the raw pointers, the atomics and `Mouse` own
  nothing.
- **proof (soundness)**: `.port/phase-2/mirihar-n444` (non-FFI model of the two blocks: the render thread
  writes them through the raw roots while the main thread retags the whole surface, then `deinit` runs
  inside `&mut self` after the join): `n502_teardown_glueless_leak` (the old teardown) LEAK under SB and
  TB, `n502_teardown_drop_and_dealloc` (the fix) CLEAN under SB and TB, 16 seeds each.
- **regression test**: G-LSAN on macOS over `rustty-apprt-c` (`macos_window_host`), RED before the fix;
  Apple `leaks` on the native `macos_window_host` binary after the fix: 0 leaks (was 83). Native
  `rustty-core` lib 79 passed, `macos_window_host` 18 passed.

## N503 — rustty-renderer's macOS test helpers leaked their renderers and font grids (N411's macOS twins)

- **id**: N503 (provisional; P7-W16a). **severity**: low (tests only). **state**: CLOSED 2026-10-02.
- **found**: G-LSAN's first macOS run (P7-W16a), `rustty-renderer` lib tests. N411 fixed the Linux
  helpers (`generic/linux_impl/tests.rs`: `free_test_font_grid`, `deinit_test_renderer`) under memcheck
  and recorded that the macOS twins leak the same way with no leak detector to report them. G-LSAN did:
  direct leaks under `generic/macos_impl/tests.rs` `test_font_grid_and_size` (21), `golden_renderer` (20)
  and `test_renderer` (20), about 38 KB direct and 51 MB indirect (the grids' atlases, the shaper cache,
  the terminals' pages), plus three terminals that `lifecycle::offscreen_resize::draw_one_frame` and
  `mailbox::update_and_draw` never `deinit`ed.
- **fix**: `generic/macos_impl/tests.rs` gains the Linux helpers' twins, `free_test_font_grid` (grid
  `deinit`, the box, the library) and `deinit_test_renderer` (`thread_exit`, `deinit`, then the grid:
  the reference tests' `defer` order). Every macOS test that builds a renderer from `test_renderer` /
  `golden_renderer` / `test_font_grid_and_size` ends with it (the existing
  `thread_exit`/`deinit` pairs became the helper call), the two helpers that return early
  (`golden.rs` `draw_center`, `lifecycle.rs` `prep_with_bg_file`) call it before returning, and
  `draw_one_frame` / `update_and_draw` `deinit` their terminals.
- **evidence**: native `rustty-renderer` lib 140 passed; Apple `leaks` on that binary: 0 leaks; G-LSAN
  on macOS `rustty-renderer`: only S15/S16 (ColorSync caches) remained, now suppressed.

## N506 — macOS ASan runtime can deadlock in `fork` while another thread frees: an intermittent hang in G-ASAN / G-LSAN

- **id**: N506 (provisional; P7-W16a fix round 2). **severity**: low (sanitizer lanes only; no product
  defect). **state**: OPEN (mitigated: the gate now bounds each crate and reports a hang RED).
- **found**: one Mac G-LSAN pass under heavy load (load average ≈ 30) hung for an hour in
  `rustty-apprt-c`'s `tests/macos_window_host.rs` at ~740% CPU. Sample: an io thread in
  `Exec::threadEnter` → `Subprocess::start` → `Command::startPosix` → `fork` → `libSystem_atfork_prepare`
  → `_malloc_fork_prepare` → `__sanitizer::StaticSpinMutex::LockSlow`, and a libdispatch worker in
  `wrap_free` → `__asan::Allocator::QuarantineChunk` → `StaticSpinMutex::LockSlow`, both spinning, with
  the remaining test threads waiting behind them (`os_unfair_lock`, `pthread_join` in
  `Surface::deinit`). No Rustty frame holds a lock: the two spinning threads are inside the ASan
  runtime's allocator/quarantine locking around `fork` (the mechanism of LLVM D33325's
  `BeforeFork` allocator locking), a lock-order problem in the runtime, not in the port.
- **which test forks**: every `macos_window_host` test that creates a surface. `Surface::init` starts
  the io thread, whose `Exec` backend spawns the shell with `fork` (`crates/rustty-termio/src/command.rs`
  `startPosix`, `libc::fork()`, the reference's `Command.zig` `fork()`), while the binary's other
  test threads, their surfaces' render/io threads and libdispatch workers allocate and free.
- **rate**: 1 hang in about 33 runs of that binary under the sanitizer (the hung pass; 6 other full
  passes; 2 re-runs; 25 back-to-back runs on 2026-10-02 at load ≈ 12, all GREEN, 51 tests each).
  Load-dependent and intermittent.
- **mitigation (applied)**: `sanitizers.sh` bounds every crate (`SAN_CRATE_TIMEOUT_S`, default 1200 s;
  2400 s for MSan): past it the process tree is killed by explicit PIDs and the crate is RED with "HANG
  after Ns (no verdict)", the pass continues, and a hang can never be GREEN (G-SAN-SELF arm).
- **proposed fix (not applied)**: (a) under `cfg(rustty_asan)` on macOS, have `Command::startPosix`
  spawn with `posix_spawn` instead of `fork` (no atfork handlers run in the parent). That is a product
  code change behind a sanitizer cfg and drifts from the reference's `fork`, so it needs a ruling;
  (b) test-only: run `macos_window_host` with `--test-threads=1` under the sanitizer lanes, which
  narrows the window (fewer concurrent frees) but cannot close it (each surface's own render/io threads
  and libdispatch still free during the fork), so it is not clearly correct and was not applied.
