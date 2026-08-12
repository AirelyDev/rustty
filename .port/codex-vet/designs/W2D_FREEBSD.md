# W2D — FreeBSD enablement to type-check parity (F-004 xev/kqueue, F-003 renderer/OpenGL, F-002 apprt/GTK)

STATUS: DESIGN — ready for fix agents. Wave w2. 2026-08-10.

Findings: `.port/codex-vet/STATE.json` → `.vet` of **F-004** (xev kqueue macOS-only),
**F-003** (OpenGL renderer Linux-only), **F-002** (GTK apprt Linux-only). All three
confirmed, previously deferred-by-tier; user directive: fix now, to **type-check
parity** (a FreeBSD build must resolve every name the oracle resolves; runtime
bring-up on a real FreeBSD host is a documented follow-up, §V4).

Oracle pins (READ-ONLY):

- `GHOSTTY=/Users/gael/projects/ghostty` @ 6ad1fe7d.
- The pinned vendored libxev (the package the D6 port was written against):
  `XEV=$GHOSTTY/zig-pkg/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F`
  — every kqueue cite below is `$XEV/src/...`.
- libc crate facts verified against the resolved `libc-0.2.189`
  (`~/.cargo/registry/.../libc-0.2.189/src/unix/bsd/freebsdlike/freebsd/`):
  default ABI is **freebsd12** (`build.rs:111-125`), so `libc::kevent` =
  `{ident: uintptr_t, filter: c_short, flags: c_ushort, fflags: c_uint, data: i64,
  udata: *mut c_void, ext: [u64;4]}` (`freebsd12/mod.rs:23-31`) — byte-matching the
  Zig `std.c.Kevent` freebsd arm the oracle selects at `$XEV/src/backend/kqueue.zig:1863`.

Doctrine: `.port/PORT_RULES.md` R1 (bug-for-bug fidelity — including the oracle's
own FreeBSD wakeup stub, WI-4), R3 (no new deps — everything below rides `libc`),
R5 (additive; the macOS/Linux lanes must stay byte-identical — every change is a
cfg-widening or a new cfg(freebsd) arm, never an edit inside an existing arm).

Design fork, resolved up front (memory: discuss-tradeoffs-before-recommending):
for every gate currently spelled `target_os = "linux"` there were two options —
(a) introduce a `freebsd`-named alias cfg via build.rs, (b) widen in place to
`any(target_os = "linux", target_os = "freebsd")`. **(b) is chosen** everywhere:
it is the exact shape the tree already uses for multi-OS arms
(`any(target_os = "macos", target_os = "ios")` in backend.rs), it needs no
build.rs machinery, and it keeps `grep`-ability (the vet located every gate by
grepping the literal cfg — the fix must remain locatable the same way). Same for
`target_os = "macos"` gates in rustty-xev → `any(target_os = "macos", target_os
= "freebsd")`. iOS/visionos are explicitly OUT of scope (F-004 vet notes their
sibling mismatch; widening to them would compile the mach paths untested — leave
for the WI-13 ledger row).

Wave-1 collision map (ADVERSARY ADDITION — read before editing): fix wave w1 is
CONCURRENTLY editing files this design touches. Per `lanes-w1.json`:
`kqueue.rs` (w1-L07-b01 layout/abi asserts, w1-L07-b03 timer/watcher asserts),
`async.rs` (w1-L07-b03), `rustty-apprt/src/lib.rs` (w1-L06-b02),
`rustty-apprt/Cargo.toml` (w1-L04-b01), `rustty-renderer/src/generic.rs` +
`generic/linux_impl.rs` (w1-L08-b01/b02/b03). Every rustty `file:line` in this
doc is an anchor as of d34cc15: fix agents MUST re-locate each site by the
quoted code/grep pattern, not the line number, and rebase on the w1 result if
those batches land first. Oracle cites are at the immutable pin and stay exact.

---

## §V — Verification reality and the check lanes (read before any work item)

Measured on this host (macOS, 2026-08-10):

- `cargo check --target x86_64-unknown-freebsd -p rustty-xev` — **GREEN today**
  (crate falls into `no_backend_seam`). This is the primary F-004 gate: it must
  stay green after every WI while the seam is replaced by the real kqueue surface.
- `cargo check --target x86_64-unknown-freebsd -p rustty-vt` — RED at
  `rustty-os` (`libc::IUTF8` missing on freebsd — real type error, WI-1) once the
  `cc` build scripts are stubbed (below); the vendored-C `-sys` crates otherwise
  die in `cc`/bindgen for lack of FreeBSD headers.
- Deep crates (renderer/termio/core) additionally hit bindgen in
  `glslang-sys`/`sentry-sys`/`wuffs-sys`/`libxml2-sys`/`freetype-sys`
  (`fatal error: 'stdlib.h' file not found` / sentry.h "unsupported platform")
  — bindgen needs real FreeBSD headers; note `sentry-sys/build.rs:111,180`
  already carries freebsd arms, so with headers these are expected to pass.
- gtk4-rs `-sys` crates probe **pkg-config** for the target — no local fix short
  of a full FreeBSD GTK install; the gtk-featured apprt is the one permanently
  host-unverifiable lane (§V4).

Three lanes, in order of strength:

**V1 — shallow FreeBSD cross-check (works today, no downloads).**
A stub cross toolchain unblocks `cc`-only build scripts for *check* builds
(objects are never linked by `cargo check`; cc-rs only requires exit 0 and an
output file). Prototyped and verified this session: a `stub-cc` that answers
`--version` like clang and otherwise touches the `-o` path, plus a `stub-ar`
that touches every `*.a` argument, exported as
`CC_x86_64_unknown_freebsd`/`CXX_x86_64_unknown_freebsd`/`AR_x86_64_unknown_freebsd`.
Covers: `rustty-xev` (no stub even needed), `rustty-global`,
`rustty-build-config`, and — after WI-1 — `rustty-os`, `rustty-simd`,
`rustty-vt` (highway-sys has pre-generated bindings; its C++ stubs out clean).

**V2 — full FreeBSD cross-check (sysroot mode; provision once).**
With a real FreeBSD sysroot, clang cross-compiles the vendored C for real and
bindgen parses real headers — everything except pkg-config crates checks:
```sh
curl -LO https://download.freebsd.org/releases/amd64/14.3-RELEASE/base.txz
mkdir -p ~/toolchains/freebsd14-sysroot && tar -C ~/toolchains/freebsd14-sysroot \
    -xf base.txz ./usr/include ./usr/lib ./lib
export FREEBSD_SYSROOT=~/toolchains/freebsd14-sysroot
# gate exports: CFLAGS_x86_64_unknown_freebsd="--sysroot=$FREEBSD_SYSROOT"
#               CXXFLAGS_x86_64_unknown_freebsd="--sysroot=$FREEBSD_SYSROOT"
#               BINDGEN_EXTRA_CLANG_ARGS_x86_64_unknown_freebsd="--sysroot=$FREEBSD_SYSROOT"
```
Covers additionally: `rustty-termio`, `rustty-renderer`, `rustty-core`,
`rustty-cli`, `rustty-apprt` (WITHOUT `gtk`), `rustty` bin
(`--no-default-features`).

**V3 — Linux regression (rvm).** rsync the tree to `rvm:~/rustty-vmwork`
(pattern: `.port/tools/gates/ws19_portal_openuri_gate.sh`) and run
`cargo check --workspace` + `cargo test -p rustty-apprt --features gtk,wayland,x11`
— proves every `any(linux, freebsd)` widening left the linux arm byte-working.
Host macOS `cargo test --workspace` is the R5 regression for the macos arms.

**V4 — what stays unverifiable here, and the de-risk.**
(1) gtk-featured `rustty-apprt`/`rustty` bin on FreeBSD (pkg-config): correct
by construction — every widened site is the *exact* linux pattern with an
`any()` around the OS; gtk4-rs itself supports FreeBSD via pkg-config (the
crates make no OS assumption beyond pkg-config discovery; FreeBSD ports ships
gtk4, libadwaita, gtk4-layer-shell). (2) Any *runtime* behavior (kqueue loop
run, GTK launch, EVFILT_PROC delivery). Both go into an explicit DEFERRALS.md
FreeBSD row (WI-13) with the concrete follow-up: run V1/V2's command set +
`cargo test -p rustty-xev` + `cargo run -p rustty` on a real FreeBSD 14 env
(VM or jail), mirroring how the pinned libxev itself gates FreeBSD
(`$XEV/src/main.zig:115-117` runs the whole Kqueue suite on freebsd).

**WI-0 — the gate script (build FIRST; every later WI cites it).**
File: `.port/tools/gates/freebsd_check_gate.sh` (new; follow the header/negative-
control style of `ws19_portal_openuri_gate.sh`). Behavior:
- writes the V1 stub toolchain to a temp dir (stub-cc: answer `*--version*` with
  a clang banner, else locate the arg after `-o` and `: > "$out"`; stub-ar:
  `: >` every `*.a` arg), exports the three `*_x86_64_unknown_freebsd` env vars;
- if `FREEBSD_SYSROOT` is set and exists, switches to V2 (real `cc` +
  `CFLAGS_…`/`BINDGEN_EXTRA_CLANG_ARGS_…` sysroot exports, no stubs);
- runs, in order, failing fast:
  `cargo check --target x86_64-unknown-freebsd -p rustty-xev --all-targets`,
  then `-p rustty-os -p rustty-vt` (V1+), then (V2 only)
  `-p rustty-termio -p rustty-renderer -p rustty-core -p rustty-cli`,
  `-p rustty-apprt` (no features), `-p rustty --no-default-features`;
- anti-vacuity arm: greps its own cargo output to assert the freebsd target
  triple actually appeared (a mis-set target silently checking the host is the
  gate's silent-green hazard — cf. memory `derived-gate-green-can-be-bookkeeping`).
Check: run it before any code change — it must fail exactly at the WI-1 IUTF8
error (V1) — that is the gate's own negative control; record the output in the
commit message.

---

## §A — Prerequisite

**WI-1 — rustty-os: FreeBSD `IUTF8`.**
File: `crates/rustty-os/src/pty.rs:267` (`attrs.c_iflag |= libc::IUTF8;`).
Oracle: `$GHOSTTY/src/pty.zig:177` (`attrs.c_iflag |= c.IUTF8;`) where `c` is the
translate-C module over the *target's* `termios.h`
(`src/build/SharedDeps.zig:235` "pty-c") — i.e. the oracle compiles against
whatever the FreeBSD header defines. `libc` 0.2.189 has **no** `IUTF8` for
freebsd (verified by grep of `freebsdlike/`). Fix: a crate-local shim next to the
use site:
```rust
// PORT NOTE (W2D): `c.IUTF8` resolves through pty-c to the TARGET's termios.h
// (SharedDeps.zig:235). libc lacks the constant for freebsd; bind it locally.
#[cfg(not(target_os = "freebsd"))]
use libc::IUTF8;
#[cfg(target_os = "freebsd")]
const IUTF8: libc::tcflag_t = 0x00004000; // FreeBSD sys/sys/_termios.h:100 (releng/14.3), verified below
```
Value VERIFIED (adversary pass, 2026-08-10):
`curl -s 'https://cgit.freebsd.org/src/plain/sys/sys/_termios.h?h=releng/14.3' | grep IUTF8`
→ `#define IUTF8 0x00004000 /* assume input is utf-8 encoded */` (line 100).
So the oracle's `c.IUTF8` resolves on FreeBSD ≥ 14 and 0x00004000 is the exact
constant; the shim is a straight bind, no contingency arm needed. PORT NOTE
should record that the header evidence is releng/14.3 (libc's freebsd12 ABI pin
does not carry the constant, hence the local bind).
Check: `freebsd_check_gate.sh` now passes `-p rustty-os -p rustty-vt` (V1).

---

## §B — F-004: rustty-xev FreeBSD kqueue backend

Porting order inside §B: WI-2 (ABI core) → WI-3 (machport gating) → WI-4 (wakeup)
→ WI-5 (Async watcher) → WI-6 (futex) → WI-7 (module/seam/gate widening — the
step that actually flips FreeBSD off the `no_backend_seam`) → WI-8 (tests/parity).
`crates/rustty-xev/src/backend.rs` needs **no change** — its freebsd→Kqueue arms
(:48-50, :101-103) are already faithful to `$XEV/src/backend.zig:20,38`.
Run `cargo check --target x86_64-unknown-freebsd -p rustty-xev` after EVERY WI;
WI-2..6 land dead cfg(freebsd) code behind the still-macos module gates, so the
crate must stay green throughout (the code only goes live at WI-7).

**WI-2 — kqueue.rs kevent ABI arms.** File: `crates/rustty-xev/src/kqueue.rs`.

1. `:119` `pub type Kevent = libc::kevent64_s;` → the oracle's switch
   (`$XEV/src/backend/kqueue.zig:1861-1865`):
   ```rust
   #[cfg(target_os = "macos")] pub type Kevent = libc::kevent64_s;
   #[cfg(target_os = "freebsd")] pub type Kevent = libc::kevent;
   ```
2. `:123-131` `KeventInit` stays exactly as is (it is already the portable
   `posix.Kevent` literal shape — `ident: u64, filter: i16, flags: u16,
   fflags: u32, data: i64, udata: u64`).
3. `:137-148` `kevent_init` grows the freebsd arm (`kqueue.zig:1932-1944`; the
   macos body is untouched). Freebsd arm uses zeroed-init so the code never
   names `ext` (compiles against both libc freebsd11/12 ABIs):
   ```rust
   #[cfg(target_os = "freebsd")]
   #[inline]
   fn kevent_init(ev: KeventInit) -> Kevent {
       // PORT NOTE: `if (Kevent == std.c.Kevent) return ev;` — the freebsd
       // identity arm (kqueue.zig:1935). Field-width adaptation only.
       let mut k: Kevent = unsafe { core::mem::zeroed() };
       k.ident = ev.ident as libc::uintptr_t;
       k.filter = ev.filter;
       k.flags = ev.flags;
       k.fflags = ev.fflags;
       k.data = ev.data;
       k.udata = ev.udata as *mut libc::c_void;
       k
   }
   ```
4. `:150-215` `kevent_syscall` — add the freebsd `libc::kevent` arm
   (`kqueue.zig:1876-1899`): identical errno map
   (EACCES→AccessDenied, EINTR→continue, ENOENT→EventNotFound,
   ENOMEM→SystemResources, ESRCH→ProcessNotFound; EFAULT/EBADF/EINVAL
   unreachable), `c_int` counts via the same `try_from → Overflow` guard,
   `timeout: *const libc::timespec` passed straight through (no flags/ext args).
   The macos `kevent64` arm is untouched.
5. Kevent field reads: the tick/tick_inner loops read `ev.udata`, `ev.flags`,
   `ev.data` (sites around `:566-590` and `:950-995`) — `udata` is `u64` on
   macos, `*mut c_void` on freebsd. Add two per-target inline accessors next to
   `kevent_init` and route ALL reads through them (grep `\.udata` /
   `\.flags &` in kqueue.rs for the full site list):
   ```rust
   #[inline] fn kev_udata(ev: &Kevent) -> u64            // macos: ev.udata; freebsd: ev.udata as u64
   #[inline] fn kev_set_udata(ev: &mut Kevent, v: u64)   // macos: ev.udata = v; freebsd: ev.udata = v as *mut c_void
   #[inline] fn kev_flags(ev: &Kevent) -> u16            // identity on both (c_ushort == u16)
   ```
   (`ev.data` is `i64` on both — freebsd12 ABI — no accessor needed; `.flags`
   *assignment* also compiles on both since both are u16-typed, so only the
   `udata` field needs the setter.)
6. Kevent handling outside `kevent_init`, two distinct shapes (adversary
   correction — the disarm sites are NOT literals):
   a. The fresh `Kevent { .. }` literals in the read/write/proc arms of
      `Completion::kevent` (`:1742-1813`): rewrite each to a
      `KeventInit { .. }` + `kevent_init(..)` call — exactly the oracle's own
      shape (`kqueue.zig:1063-1131`, every non-machport arm is
      `kevent_init(.{...})`) — removing all `kevent64_s`-fielded literals from
      shared code. (The `:509` disarm consumes `Completion::kevent` output and
      only sets `.flags = libc::EV_DELETE` — fine on both as-is.)
   b. The copy-then-mutate disarm sites `:855`/`:988`
      (`events[changes] = ev; events[changes].flags = libc::EV_DELETE;
      events[changes].udata = 0;`): keep the copy and the `.flags` assignment
      (u16 on both), and route ONLY the udata zeroing through
      `kev_set_udata(&mut events[changes], 0)` — the oracle mutates the fields
      of the copied kernel event in place the same way; only libc's
      `*mut c_void` udata type forces the accessor.
   The macos-only machport/Wakeup literals (WI-3/WI-4) keep `ext`.
7. `:96` `NOTE_EXIT_FLAGS` → two arms (`kqueue.zig:45-49`):
   ```rust
   #[cfg(target_os = "macos")]
   pub const NOTE_EXIT_FLAGS: u32 = libc::NOTE_EXIT | libc::NOTE_EXITSTATUS;
   #[cfg(target_os = "freebsd")]
   pub const NOTE_EXIT_FLAGS: u32 = libc::NOTE_EXIT;   // libc freebsd/mod.rs:2265
   ```
   The proc `perform` decode (`:2016-2030` region) is SHARED, unchanged — the
   oracle shares it too (`kqueue.zig:1269-1283`): on FreeBSD `NOTE_EXIT`
   delivery puts the wait status in `ev.data`, and the port's own portable
   helpers `libc_wifexited`/`libc_wexitstatus` (kqueue.rs:2692-2695) decode it
   identically (they are plain bit tests, no libc macro dependency). The
   `(*ev).data` read there is `i64` on both ABIs — no accessor needed, but it
   must NOT be routed through `kev_udata` by an over-eager grep.
8. `:74-93` `available()` — VERIFY-ONLY: the ported comment block already
   carries the oracle's freebsd arm text (`// .freebsd => true`, kqueue.zig:35)
   and the body returns `true`; after WI-7 freebsd compiles it and correctly
   reports available. No edit.
9. Timer semantics: **no freebsd divergence exists** — timers ride the intrusive
   heap, not EVFILT_TIMER (`kqueue.zig:81-84`), the kevent-wait timeout is a
   plain `libc::timespec` computed identically (`kqueue.zig:483-505`), and
   `Timespec = libc::timespec` (`:62`) has the same i64/i64 shape on freebsd.
   `timer.rs` is gate-widening only (WI-7). Audit step while here: the `accept`
   perform path's `SOCK_CLOEXEC` fcntl emulation (`:100`) — FreeBSD has native
   `accept4`; keep the emulation (it is what the shared oracle code does via
   `posix.accept`) and note it.

Cite discipline: every new arm carries a `// PORT NOTE:` quoting the
`$XEV/src/backend/kqueue.zig` line it ports, same style as the existing macos
arms. Check: `cargo check --target x86_64-unknown-freebsd -p rustty-xev` (still
green — code is dead until WI-7); host `cargo test -p rustty-xev` (R5).

**WI-3 — kqueue.rs machport op gating.** File: `crates/rustty-xev/src/kqueue.rs`.
Oracle: the `machport` payload is `if (!builtin.os.tag.isDarwin()) void else
struct {...}` (`kqueue.zig:1674`) and `Completion.kevent()`'s machport arm is a
comptime `return null` off-Darwin (`kqueue.zig:1081`). Port:
- `:2345-2350` — split the variant:
  ```rust
  #[cfg(target_os = "macos")]
  Machport { port: darwin::mach_port_name_t, buffer: ReadBuffer },  // (existing fields verbatim)
  #[cfg(target_os = "freebsd")]
  Machport,          // PORT NOTE: `machport: if (!isDarwin) void` (kqueue.zig:1674)
  ```
- match arms at `:1267` (start action), `:1758-1780` (kevent construction —
  freebsd arm `=> None`, kqueue.zig:1081), `:2014` (perform), `:2134-2137`
  (syscall_result): keep the macos bodies under `#[cfg(target_os = "macos")]`
  and add one-line freebsd arms (`unreachable!()` where the oracle's op cannot
  be constructed off-Darwin, `None` for the kevent arm).
- `Result::Machport` (`:2383`) and `MachPortError` (`:2444`) stay
  UNCONDITIONAL — Zig error sets/result unions are platform-free in the oracle
  (`kqueue.zig:1701`), and the macOS re-export list keeps working unchanged.
- `:36` `use crate::darwin;` → `#[cfg(target_os = "macos")]` (all remaining
  `darwin::` uses are inside macos-gated items after WI-3/WI-4).
Check: as WI-2.

**WI-4 — kqueue.rs Wakeup: the oracle's FreeBSD stub, bug-for-bug.**
File: `crates/rustty-xev/src/kqueue.rs` `:2182-2245` + the thread-pool wakeup
plumbing (`:1368`, `:1440-1495`, Completion field defaults `:1630`).
Oracle: the non-Darwin `Wakeup` is a deliberate stub —
`$XEV/src/backend/kqueue.zig:1488-1510`: init/deinit/setup are no-ops and
`wakeup()` is `@panic("wakeup not implemented on this platform")`, with the
comment "TODO: We should use eventfd for FreeBSD. Until this is implemented,
loop wakeup will crash on BSD." **Port the stub, not an invented eventfd wakeup**
(R1). CRITICAL fidelity fact (adversary-verified, kqueue.zig:965-968): the
pool-completion path's wakeup call is inside
`if (comptime builtin.target.os.tag.isDarwin())` — on FreeBSD the oracle pushes
to `thread_pool_completions` (:963) and NEVER calls wakeup; the completion is
simply collected on the next tick. The panic stub is only reachable through an
explicit `Loop.wakeup()` (:972-975 → wakeup_state.wakeup()), which no oracle
code path invokes off-Darwin. A freebsd port whose thread_perform panics after
the push would crash every pool task — a behavior the oracle does NOT have.
Concretely:
- Existing `struct Wakeup`/`impl` (`:2193-2248`) become
  `#[cfg(target_os = "macos")]`; add the freebsd twin: a fieldless
  `struct Wakeup;` with `init() -> Ok(Wakeup)`, `deinit(&mut self) {}`,
  `setup(&mut self, _kqueue_fd: fd_t) -> Ok(())`, and
  `fn wakeup(&mut self) -> ! { panic!("wakeup not implemented on this platform") }`
  — oracle comments carried verbatim (R3). (Reached only via `Loop::wakeup`,
  mirroring kqueue.zig:1506-1509.)
- The WS19-T10 restructure (`(*c).task_wakeup = self.wakeup_state.mach_port`,
  `:1368`; notify in `thread_perform`, `:1486-1494`) is macOS-shaped. Port the
  oracle's comptime gate as real cfgs: put the `task_wakeup: Async` field
  (`:1573`), its `Default` arm (`:1629-1630` `darwin::MACH_PORT_NULL`), the
  `:1368` assignment, and the `:1489` `let _ = (*c).task_wakeup.notify();` all
  under `#[cfg(target_os = "macos")]` — on freebsd `thread_perform` is
  push-only, exactly `if (comptime isDarwin())` elided (kqueue.zig:965-968).
  (If cfg-ing the field disturbs the `:2827` size-assert commentary, gate that
  assert per-target too and note it.) Do NOT add a panicking freebsd notify
  handle — that would invent a crash the oracle does not perform. Document as
  the freebsd twin of the `[R1-DIVERGENCE, WS19 T10]` note.
- `Loop::init/deinit/tick` call `Wakeup::init/deinit/setup` identically on both
  targets (the freebsd setup no-op means no EVFILT_MACHPORT filter is ever
  registered — oracle kqueue.zig:1502-1505).
Check: as WI-2, plus host `cargo test -p rustty-xev` and the mirihar suites are
untouched (no macos-arm edits — verify with `git diff` scoped to cfg additions).

**WI-5 — async.rs: `AsyncEventFd` (the FreeBSD Async watcher).**
File: `crates/rustty-xev/src/async.rs`. Oracle: `$XEV/src/watcher/async.zig`
`:22-26` — `.kqueue => if (isDarwin) AsyncMachPort else AsyncEventFd` — and the
`AsyncEventFd` body `:33-215`. Port, in the same file (mirroring the oracle's
one-file layout):
- Wrap the existing `AsyncMachPort` content (`struct Async` `:110` etc.) in
  `#[cfg(target_os = "macos")]` — INCLUDING the darwin-flavored top-of-file
  `use` items (`use crate::darwin;`, the `MachPortError`/`Operation::Machport`
  imports at `:29-30`) and the `#![allow(deprecated)]` note, which are
  macos-only after the split; add under `#[cfg(target_os = "freebsd")]`:
  ```rust
  pub struct Async { fd: fd_t }
  pub type WaitError = crate::kqueue::ReadError;   // `pub const WaitError = xev.ReadError` (async.zig:38)
  ```
- `init()`: `libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK)`
  (libc freebsd `eventfd` `:4992`, `EFD_CLOEXEC=0x100000` `:3497` — the very
  values the oracle hardcodes in its own local extern, async.zig:44-58; PORT
  NOTE the extern→libc mapping, R3-clean since libc already binds it).
  TODO-error-handling in the oracle (`// TODO: error handling`) — mirror by
  mapping `fd < 0` to `Error` (the crate's unified init error), noted.
- `deinit()`: `libc::close(self.fd)` (`xev_posix.close`, async.zig:76-78).
- `notify()`: write the host-order `u64` 1 as 8 bytes; `EAGAIN` → `Ok(())`
  ("if the eventfd write would block (EAGAIN) then we assume success because the
  eventfd is full", async.zig:200-212); other errno → `NotifyError`.
- `wait()`: the `waitRead` arm (async.zig:89-133): build the completion with a
  `read` op on `self.fd` and an array `ReadBuffer` (the crate's existing
  `ReadBuffer::Array` — same one the macos machport path uses), trampoline
  mapping `Ok(n) => { debug_assert!(n > 0); Ok(()) }`, `Err(e) => Err(e)`.
- Module doc: note both arms now live here, exactly as `watcher/async.zig` does.
The kqueue Loop does NOT consume this type on freebsd (WI-4's Wakeup is the
stub); consumers are termio (`renderer_wakeup`) exactly as on macOS.
Check: as WI-2 (dead until WI-7); after WI-7 this surface is what makes
`rustty-termio`'s `xev::Async` resolve.

**WI-6 — thread_pool.rs: the FreeBSD futex arm.**
File: `crates/rustty-xev/src/thread_pool.rs` `:939-1131`. The current dispatch
gives freebsd the `SingleThreadedImpl` arm (`:1098/:1115` —
`not(any(linux, macos, windows))`), which is WRONG for a multithreaded target
(its `futex_wait` is `unreachable!("deadlock detected")` — first pool sleep
would abort). Oracle: Zig `std/Thread/Futex.zig` dispatch chain (Impl switch,
same file the existing arms' PORT NOTEs cite at :105/:132) has a dedicated
`FreebsdImpl` between the Linux and OpenBSD arms — in the 0.15.2 std it is
`Futex.zig:303-356`: `wait` = `_umtx_op(@intFromPtr(&ptr.raw),
UMTX_OP.WAIT_UINT_PRIVATE, expect, tm_size, tm_ptr)` (:319-325; the pool's
no-timeout monomorphization makes tm_size=0, tm_ptr=null) and `wake` =
`_umtx_op(..., UMTX_OP.WAKE_PRIVATE, @as(c_ulong, max_waiters), 0, 0)`
(:340-355). NOTE (adversary-verified): unlike `LinuxImpl`, `FreebsdImpl.wake`
does **NOT** clamp `max_waiters` — the val slot is `c_ulong`, not the futex
syscall's C `int`, so the linux arm's i32::MAX clamp must NOT be copied here
(bug-for-bug R1). Port:
```rust
#[cfg(all(target_os = "freebsd", not(miri)))]
fn futex_wait(atomic: &AtomicU32, expect: u32) {
    // rc ignored: SUCCESS/INTR (spurious wake) both just return — the caller
    // re-checks state, exactly the wait loop contract (Futex.zig:327-337);
    // same rc-discipline as this file's linux arm.
    unsafe { libc::_umtx_op(atomic.as_ptr() as *mut libc::c_void,
        libc::UMTX_OP_WAIT_UINT_PRIVATE,            // libc freebsd/mod.rs:4219
        expect as libc::c_ulong, core::ptr::null_mut(), core::ptr::null_mut()); }
}
#[cfg(all(target_os = "freebsd", not(miri)))]
fn futex_wake(atomic: &AtomicU32, n: u32) {
    // NO clamp: FreebsdImpl.wake passes max_waiters straight through as
    // c_ulong (Futex.zig:340-347) — the linux clamp is a c_int-slot artifact.
    unsafe { libc::_umtx_op(atomic.as_ptr() as *mut libc::c_void,
        libc::UMTX_OP_WAKE_PRIVATE,                 // libc freebsd/mod.rs:4220
        n as libc::c_ulong, core::ptr::null_mut(), core::ptr::null_mut()); }
}
```
(`libc::_umtx_op` signature: `freebsd/mod.rs:5003-5009` — `(obj: *mut c_void,
op: c_int, val: c_ulong, uaddr: *mut c_void, uaddr2: *mut c_void) -> c_int`;
Zig passes the two tail args as usizes, 0/null ≡ null_mut here.)
And add `target_os = "freebsd"` to BOTH single-threaded not-lists (`:1098`,
`:1115`) so freebsd falls out of that arm. Update the `:1090-1096` comment
("every target left over here is single-threaded") accordingly.
Check: as WI-2; this compiles on freebsd immediately (thread_pool is ungated).

**WI-7 — lib.rs: flip FreeBSD onto the kqueue surface.**
File: `crates/rustty-xev/src/lib.rs`. Every edit is a cfg text change; the
freebsd export LIST is IDENTICAL to the macos one (all names exist after
WI-2..5 — `MachPortError` et al. stay defined, WaitError aliases per-target):
1. `:89-92` `pub fn backend()` gate → `any(target_os = "macos", target_os =
   "freebsd")` (freebsd is the same single-candidate static-kqueue collapse,
   `$XEV/src/backend.zig:38`).
2. Module gates → `any(macos, freebsd)`: `async_watcher` (:180-182), `kqueue`
   (:187-189), `looppkg` (:190-192), `net` (:193-195), `process` (:200-201),
   `stream` (:207-208), `timer` (:225-226). `darwin` (:164-166) STAYS
   macos-only. (`net.rs`'s "darwin sockaddr arm" is the shared BSD layout —
   `sa_family_t: u8` + `sin_len` — identical on freebsd; add a PORT NOTE line.)
3. Re-export block `:290-341`: widen EVERY `#[cfg(target_os = "macos")]`
   attribute in the block to `any(macos, freebsd)` — 10 at d34cc15, but
   enumerate by `grep -n 'target_os = "macos"' src/lib.rs` over the block, do
   not trust the count (w1 churn) — (Async, the `kqueue::{...}` list,
   `looppkg::{...}`, `Process`, `stream::{...}`, `ThreadPool`, `Timer`,
   `TimerRunError`, `WaitError`, `ProcessWaitError`).
4. `:467` `api_tests` gate → `all(test, any(macos, freebsd))`.
5. `:563` `#[cfg(not(any(macos, linux)))] pub use thread_pool::ThreadPool;` →
   add `target_os = "freebsd"` to the not-list (its positive twin widened in 3).
6. `:611` + `:619` `no_backend_seam` not-lists → add `target_os = "freebsd"`
   (freebsd no longer falls through; the seam text stays true for the remaining
   targets). Update the seam's doc block (`:595-610`) one line: freebsd now has
   the real backend.
7. `:704-717` `backend_accessor_tests` macos arm → `any(macos, freebsd)`.
8. `tests/surface_parity.rs`: add freebsd — `:40`, `:186`, `:237`
   `any(linux, macos)` → `any(linux, macos, freebsd)`; `:213`'s macos-static-Api
   arm → `any(macos, freebsd)` (freebsd IS the static kqueue Api, same §0
   collapse).
Check (the F-004 acceptance):
`cargo check --target x86_64-unknown-freebsd -p rustty-xev --all-targets` GREEN;
then V1 gate through `-p rustty-vt`; then (V2, or defer to the real-FreeBSD
follow-up) `-p rustty-termio` — termio's ~19 `xev::*` names (F-004 vet evidence)
must now resolve; if termio shows residual freebsd holes of its own, fix by
mirroring its macos arm cfgs (expected: none — termio consumes only the seam
names). Regressions: host `cargo test -p rustty-xev`;
`cargo check --target x86_64-pc-windows-msvc -p rustty-xev`;
`cargo check --target wasm32-unknown-unknown -p rustty-xev` (seam not-lists).

**WI-8 — xev unit-test parity on freebsd.**
File: `crates/rustty-xev/src/process.rs` — the ported twin of
`test "process wait with non-zero exit code"` gets
`#[cfg_attr(target_os = "freebsd", ignore = "oracle skip: process.zig:514 SkipZigTest on freebsd")]`
(oracle `$XEV/src/watcher/process.zig:514`). All other kqueue tests compile for
freebsd via WI-7's gates and run only on real FreeBSD hardware (V4 follow-up;
oracle precedent `$XEV/src/main.zig:115-117`). No other test edits.

---

## §C — F-003: rustty-renderer OpenGL on FreeBSD

Oracle: `$GHOSTTY/src/renderer/backend.zig:20-22` — `.metal` iff Darwin, else
`.opengl` for every native target; FreeBSD is a real GTK target at the pin
(`runtime.zig:18`). All four gate sites from the F-003 vet evidence:

**WI-9 — cfg + dep-table widening.** Files:
1. `crates/rustty-renderer/src/lib.rs:105` (`pub mod opengl`) and `:188`
   (`pub use generic::Renderer`) → `any(target_os = "linux", target_os =
   "freebsd")`; update the prose at `:100-104`/`:183-187` ("Linux" → "Linux/
   FreeBSD", cite backend.zig:20-22).
2. `crates/rustty-renderer/src/generic.rs:7609` + `:7611`
   (`mod linux_impl; pub use linux_impl::*;`) → same `any(...)`. KEEP the
   `linux_impl` module/file name (w1 is editing this tree; a rename is churn
   with zero fidelity gain) and add one PORT NOTE line: "`linux_impl` is the
   `GenericRenderer(OpenGL)` arm (renderer.zig:38 `.opengl`) — serves linux AND
   freebsd since W2D."
3. `crates/rustty-renderer/Cargo.toml:205` table header →
   `[target.'cfg(any(target_os = "linux", target_os = "freebsd"))'.dependencies]`.
   GL loader crates verified FreeBSD-clean: `gl` 0.14 is generated bindings
   loaded through `gl::load_with` (no link-time dep at all — the crate's own
   comment block `:206-214`); `khronos-egl` 6 with `dynamic` dlopens
   `libEGL.so.1` via `libloading` (full unix support; FreeBSD ships libEGL via
   libglvnd/mesa). No dep additions, no version changes.
4. `crates/rustty-renderer/build.rs:41` — the glsl step gate:
   `Ok("linux")` → `matches!(std::env::var("CARGO_CFG_TARGET_OS").as_deref(), Ok("linux") | Ok("freebsd"))`.
5. `crates/rustty-renderer/src/opengl.rs:1` module doc "(L4, Linux-only)" →
   "(L4, Linux/FreeBSD)".
`rustty-core` needs **no change**: `renderer_seam.rs` already selects the
non-macos arms by complement (`:192` vs `:209`, `:239-269` — `#[cfg(not(
target_os = "macos"))] pub struct Renderer(pub rustty_renderer::Renderer);`),
so it resolves on freebsd the moment the renderer exports — cite this in the
commit message as the "exact complement" invariant the seam's own comment pins.
Check: V2 `cargo check --target x86_64-unknown-freebsd -p rustty-renderer`
(sysroot lane; if not provisioned, record as deferred-verification in WI-13's
row and rely on: rvm `cargo check -p rustty-renderer` + host workspace tests —
the freebsd arm is textually the linux arm). `grep -rn 'target_os = "linux"'
crates/rustty-renderer/src crates/rustty-renderer/build.rs` must return only
prose/comment hits afterward.

**WI-10 — renderer follow-through check.** No code. After WI-7+WI-9, V2 lane:
`cargo check --target x86_64-unknown-freebsd -p rustty-core` — proves the whole
`core → {renderer, xev, termio}` freebsd closure type-checks (this is the
"Ghostty type-checks on FreeBSD; Rustty cannot" divergence from the three vet
evidences, closed). Windows regression guard: `cargo check --target
x86_64-pc-windows-msvc -p rustty-renderer` must be bit-identical-behavior
(no windows arm was added — F-003's Windows half is correctly out of scope per
the vet: the oracle ships no Windows GUI consumer).

---

## §D — F-002: rustty-apprt GTK on FreeBSD

Oracle: `$GHOSTTY/src/apprt/runtime.zig:14-24` (`.linux, .freebsd => .gtk`).
`rustty-build-config` is already faithful (`apprt.rs:26-36`, `lib.rs:79-80`) —
no change there. The gap is exactly the linux-only gating on the sites below.
Widening pattern everywhere: `all(target_os = "linux", feature = "gtk")` →
`all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk")` (and
the `not(...)` twins get the same inner text — the not() stays the complement).

**WI-11 — apprt crate.** Files:
1. `crates/rustty-apprt/src/lib.rs:46` — the `pub mod gtk` gate (pattern above).
2. `crates/rustty-apprt/Cargo.toml:288` `[target.'cfg(target_os = "linux")'
   .dependencies]` → `cfg(any(target_os = "linux", target_os = "freebsd"))`;
   same for the dev-dep table at `:371`. No dep additions: the whole gtk4-rs
   stack (glib/gio/gdk4/gtk4/libadwaita/gdk4-wayland/gdk4-x11/gtk4-layer-shell,
   wayland-rs) discovers libs via pkg-config and supports FreeBSD (all present
   in FreeBSD ports; wayland + x11 both exist on FreeBSD). Update the prose at
   `:22-23`, `:110`, `:127`, `:387`, `:407`.
3. `crates/rustty-apprt/build.rs:204-205` — `let linux = ...== Ok("linux")` →
   accept `"freebsd"` too (rename the local to `unixy_gtk` or similar); comment
   `:22` updated.
4. Test gate headers — mechanical widen of every
   `#![cfg(all(target_os = "linux", feature = ...))]` in
   `crates/rustty-apprt/tests/*.rs` (30 files; enumerate with
   `grep -rln 'all(target_os = "linux"' crates/rustty-apprt/tests`).
5. Prose-only: `src/gtk.rs:36` and `src/gtk/cgroup.rs:14` PORT NOTES name the
   widened gate. The cgroup/systemd path deliberately stays COMPILED on freebsd
   — the oracle compiles `gtk/cgroup.zig` for freebsd too (no os.tag guard in
   the file; runtime-guarded by the `linux-cgroup` config +
   `post_fork.zig:40` switch), and it is pure gio/glib D-Bus — no linux API.
   Same argument for post_fork/pre_exec twins: no per-file cfg is added inside
   the gtk module (grep `src/gtk/` for `target_os` — the only hits must be the
   two prose notes).

**WI-12 — bin + cli launch arms.** Files:
1. `crates/rustty/src/main_rustty.rs:286` (launch arm) and `:332` (`run_gtk`)
   → widened pattern; update the `:283` comment and the `unimplemented!` text
   at `:299-304` ("default on Linux" → "default on Linux/FreeBSD"). Oracle for
   the arm itself: `main_ghostty.zig:96-112` reached whenever
   `app_runtime != .none`, which on freebsd is `.gtk` (runtime.zig:18).
2. `crates/rustty/Cargo.toml:86` dev-dep table header → `any(...)`; comments
   `:28-33`, `:84-85`.
3. `crates/rustty/tests/gui_leaf.rs` gate → widened pattern.
4. `crates/rustty-cli/src/new_window.rs` `:24, :26, :484, :486` and
   `crates/rustty-cli/src/toggle_quick_terminal.rs` `:20, :22, :113, :116` —
   all 8 sites, positive and `not()` twins (the oracle's ipc path is
   apprt-runtime-selected, not OS-gated — `apprt/ipc.zig` has no os switch, so
   freebsd rides the gtk arm exactly like linux).
Post-condition grep (the F-002 acceptance predicate):
`grep -rn 'all(target_os = "linux", feature' crates/ --include='*.rs'` → zero
hits outside comments.
Check: host `cargo build -p rustty` + `cargo test --workspace` (R5 — macOS
takes the `.none` path, byte-identical); rvm V3 lane
(`cargo check --workspace` + `cargo test -p rustty-apprt --features gtk` +
`cargo build -p rustty` — the linux GTK arm must be untouched); V2 lane
`-p rustty-apprt` (featureless) + `-p rustty --no-default-features` +
`-p rustty-cli`. The gtk-featured freebsd build is the §V4 by-construction lane.

---

## §E — Ledger + close-out

**WI-13 — DEFERRALS.md: the explicit FreeBSD row** (both F-002 and F-003 vets
ask for it — FreeBSD was only implicitly covered by the GTK-last tier). One row
stating: (a) type-check parity LANDED (W2D: xev kqueue backend, OpenGL renderer
gates, GTK apprt gates, os IUTF8 shim); (b) STILL OPEN — runtime bring-up on a
real FreeBSD ≥ 13 host (`cargo test -p rustty-xev`, GTK launch, and the
pkg-config-dependent gtk-featured check), the oracle-faithful loop-wakeup panic
stub (kqueue.zig:1489-1491 TODO — upstream libxev has the same stub; note the
precise blast radius: pool completions are push-only on freebsd (kqueue.zig:
965-968), so the stub only fires on an explicit `Loop::wakeup()` — added
cross-thread wakeup latency, not a pool crash; an eventfd wakeup fix must land
upstream-first or be recorded as R1-divergence), and the ios/visionos
kqueue-availability nit (oracle kqueue.zig:31 includes them; rustty gates
macos+freebsd only — pre-existing, unchanged by W2D). Also note libc's
freebsd12 default ABI pin (kevent `ext[4]`, `data: i64`) and that the port never
names `ext` on freebsd so a future freebsd11 build would still check.

**WI-14 — gate wiring.** Add `freebsd_check_gate.sh` (WI-0) to the ci gate
roster next to the existing windows check lane (wherever `ci.sh`/gates run the
`x86_64-pc-windows-msvc` probe), V1 mode by default (fast, no network), with the
`FREEBSD_SYSROOT` escalation documented in the script header.

**WI-15 — memory/census hygiene.** Update `XEV_BLUEPRINT.md`'s Risks line
(":514 keep the FreeBSD posix.Kevent fast-path cfg-gated for later" — now
landed) and the `:519-521` cfg-discipline paragraph to name the
`any(macos, freebsd)` gate as the new normal for the kqueue tree.

## Summary of per-item check commands

| WI | command(s) |
|----|------------|
| 0  | `bash .port/tools/gates/freebsd_check_gate.sh` (must fail at IUTF8 — negative control) |
| 1  | gate V1 → `-p rustty-os -p rustty-vt` green |
| 2-6| `cargo check --target x86_64-unknown-freebsd -p rustty-xev` (stays green, code dead); host `cargo test -p rustty-xev` |
| 7  | `... -p rustty-xev --all-targets` green; windows + wasm32 regression checks; V2 `-p rustty-termio` |
| 8  | host `cargo test -p rustty-xev` (ignore-attr compiles) |
| 9  | V2 `-p rustty-renderer`; rvm `cargo check -p rustty-renderer`; grep post-condition |
| 10 | V2 `-p rustty-core`; windows renderer check unchanged |
| 11 | rvm `cargo test -p rustty-apprt --features gtk`; V2 `-p rustty-apprt` (no features) |
| 12 | host `cargo test --workspace`; rvm `cargo build -p rustty`; V2 `-p rustty --no-default-features -p rustty-cli`; workspace-wide grep post-condition |
| 13-15 | doc-only; `.port/tools/gates/check_comment_coverage.py` if it scopes touched files |
