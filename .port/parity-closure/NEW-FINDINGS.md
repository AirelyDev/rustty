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

  One further bypass exists outside the GTK tree:
  `crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs:418`
  (`eprintln!("info(glyf_rasterize): …")`). It is the only one, and it is capped too.

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
  through the facade must tighten it in the same commit. **188 sites are capped**: 187 in
  the six GTK files a `LOG` row cites, plus the `rustty-font` one. The remaining 57 sit in
  GTK files no `LOG` row names, so they are outside the portmap and outside the gate — a
  direct consequence of scoping the portmap to audited files, and worth widening if this
  finding is worked.

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
