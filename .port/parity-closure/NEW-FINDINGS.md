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
- **state**: **OPEN — needs an owner.** Closing it means giving
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
- **state**: **OPEN**, scheduled as its own task. Scope: give the renderer's error types
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
- **state**: **OPEN.** The gate and the corrected baseline are landed; the 183-site debt is
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
- **state**: **OPEN.** Not fixed here — this task is a gate, and adding assertions to two
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
- **state**: **OPEN.** Scope: route the GTK subtree through `rustty_core::logging::log_with`
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
- **state**: **OPEN.** The instrument is fixed and the debt is declared. Working it down is a
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
- **state**: **OPEN** as a tracked non-defect. `src/config/Config.zig`'s G-LOGFILE
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
- **state**: **OPEN** as a class. The three instances found are each annotated at their code
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
- **state**: **OPEN** as a standing check. All 12 present divergences are declared with
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

**state**: OPEN as a measurement caveat. No port change is owed.

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

**state**: OPEN as a dependency. Discharged by whoever closes `S94-F1`, in the same change.

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
- **state**: **OPEN.**

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
- **state**: **OPEN.**

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
- **state**: **OPEN**, tracked by **S204-F1** (re-opened 2026-09-11). Not attempted by task 16.
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
- **state**: **OPEN**.

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

**state**: OPEN as a watch item. Discharged if upstream gives `updateConfigEvent` a caller or a
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

**state**: OPEN. Needs a decision on the `oniguruma` feature default, and — if it stays OFF — a
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

**state**: OPEN. Needs (1) the failure reproduced under deliberate load and the failing
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

**state**: CLOSED by task R13-L3, on the R13-L2 review's Linux run (see the second update).

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

**state**: OPEN (remedy proposed, not implemented).

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

**state**: OPEN.

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
- **state**: OPEN — fix written, Linux run pending (same as S112-F8).

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
- **state**: OPEN.

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
- **state**: OPEN — fix written, Linux run pending.

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
  it fail at the `expect_approx_equal` assertion. The `fontconfig_freetype` values are
  UNVERIFIED here, because no Linux run was made.
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
- **state**: OPEN. `font` stays `PORT STATUS: in progress` until it closes.
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
- **state**: OPEN as a record until S7-4/S7-9 (S179-F2's remainder).

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
- **state**: fix written, Linux build pending.

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
- **severity**: low. **kind**: R1 (Ghostty accepts every backend on every target); one OPEN gap.
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
    Windows (WS20) and has not built it there; not attempted (no Windows host here).
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
- **state**: OPEN only for `fontconfig_freetype` on Windows; everything else in this entry CLOSED.

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
