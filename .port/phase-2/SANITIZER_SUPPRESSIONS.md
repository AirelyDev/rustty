# Sanitizer suppressions and exclusions (G-ASAN, G-LSAN, G-MSAN, G-HWASAN)

The sanitizer gates (`.port/tools/gates/sanitizers.sh --gate asan|lsan|msan|hwasan`,
registered in `.port/tools/ci.sh`; design:
`docs/superpowers/specs/2026-10-01-memory-safety-lanes-design.md` §B) run every
workspace crate's tests under AddressSanitizer, ASan + LeakSanitizer,
MemorySanitizer and (aarch64 Linux, P7-W16b) HWAddressSanitizer. This file is the register of everything those gates do NOT
check, in the same spirit as `MIRI_EXCEPTIONS.md`: each entry has a cited
reason and evidence, and nothing is suppressed or excluded silently.

## Criteria (all must hold for an entry)

1. **Not ours.** A suppression covers a finding inside a system library or
   vendored third-party C/C++, never in Rustty code. A finding in Rustty code
   is a bug: it gets an N id (and a UD when Ghostty has it too), a minimal fix
   and a regression test.
2. **Evidence.** The entry quotes the sanitizer's report (the leaking or
   faulting frames) from a named run, so a reader can see that the stack is
   entirely third-party below the call we make.
3. **Narrow.** The pattern names the third-party function or module, never a
   Rustty crate, never a whole test binary.
4. **Re-checkable.** Removing the entry and re-running the named command
   reproduces the report. A stale entry (it no longer matches anything) is
   removed.

An exclusion of a whole crate from a gate needs the same: the reason the crate
cannot run under that sanitizer at all (it does not build, or it links
uninstrumented code the sanitizer cannot model), and the evidence.

## Mechanics

- G-LSAN reads `.port/phase-2/lsan.supp` through
  `LSAN_OPTIONS=suppressions=…:print_suppressions=1`, so every suppressed leak
  is listed in the run's log. Each line in `lsan.supp` has a WHY comment above
  it (D6 rule c, `d6_cfg_mapping_audit.py`) pointing at its row here.
- Crate exclusions are the `ASAN_EXCLUDES`, `LSAN_EXCLUDES`, `HWASAN_EXCLUDES` and `MSAN_EXCLUDES`
  arrays in `sanitizers.sh`, each `"<crate>|<reason>"`. An entry naming a crate
  that is no longer a workspace member is RED. G-MSAN's scope is an allow-list:
  every member must be in `MSAN_SCOPE` or `MSAN_EXCLUDES`.

## LeakSanitizer suppressions (`lsan.supp`)

| id | pattern | library | reason | evidence |
|---|---|---|---|---|
| S1 | `leak:libEGL_mesa.so` | Mesa EGL (system, uninstrumented) | `eglInitialize` on the surfaceless platform allocates per-process display state inside Mesa that is never freed. The port, like Ghostty (`OpenGL.zig:113-115`, "Do not destroy the EGL display here as it is shared across the entire process"; DECISIONS B10), never terminates the shared display. | G-LSAN, rvm, 2026-10-01, `rustty-renderer`: 28 leak reports, every one with its first non-allocator frame in `/lib64/libEGL_mesa.so.0` (5) or `/lib64/libgallium-26.1.4.so` (23), the Rust frames below it ending at `OffscreenContext::init` (`crates/rustty-renderer/src/opengl/gl/sys.rs:214`, the `egl.initialize(display)` call), reached from `generic::linux_impl::tests::golden_renderer`. |
| S2 | `leak:libgallium` | Mesa gallium/llvmpipe driver (system) | Loaded by the same `eglInitialize`; its screen and shader-cache allocations live for the process. | as S1. The final G-LSAN run (rvm, 2026-10-02) lists what both matched (`print_suppressions=1`): S1 139 leaks, 28,395 bytes; S2 940 leaks, 132,019 bytes; all in `rustty-renderer`, nothing anywhere else. |

## Sanitizer runtime options set by the lanes

Not suppressions, but they change what a lane does, so each is recorded with its reason (also next to
the option in `sanitizers.sh`).

| option | lanes | reason | evidence |
|---|---|---|---|
| `detect_leaks=0` | G-ASAN | Apple's ASan has no leak checker (rustc rejects `-Zsanitizer=leak` on Darwin); on Linux G-LSAN is the leak lane, so G-ASAN's verdict stays memory errors. | design §B |
| `detect_leaks=0` | G-HWASAN | HWASan carries LeakSanitizer on Linux; G-LSAN is the leak lane, so G-HWASAN's verdict stays memory errors (as G-ASAN's). | P7-W16b |
| `allocator_may_return_null=1` | G-ASAN, G-LSAN, G-MSAN, G-HWASAN | ASan's malloc aborts on a request above its 1 TiB maximum, where the system malloc returns NULL; the port's out-of-memory tests rely on that NULL (`try_reserve` reporting failure, the port of Zig's `error.OutOfMemory` paths). It does turn off three ASan reports: allocation-size-too-big (a request over the allocator's maximum), calloc-overflow (count x size overflowing) and out-of-memory (the allocator failing): each now returns NULL to the caller instead of aborting with a report. Out-of-bounds, use-after-free, double-free, leak and uninitialized-read detection are unaffected, and a NULL the code does not handle still crashes visibly. | NEW-FINDINGS N496 item 5 |
| `RUST_MIN_STACK=8388608` | all four | Instrumented frames are 2-3x their native size; a test that checks a recursion bound fits the default 2 MiB stack overflows under ASan with no memory error. Unbounded recursion still overflows 8 MiB; the native bound is checked by `cargo test --workspace`. | N496 item 6 |
| line-tables-only debuginfo | all four | Full debuginfo made the GTK lane's instrumented test links exhaust the 10 GB VM (ld SIGBUS). Reports keep every function, file and line. | N496 item 7 |

## Test-binary hooks

| where | what | reason | evidence |
|---|---|---|---|
| `crates/rustty-termio/src/command.rs` tests module | defines `__lsan_is_turned_off`, non-zero only in a forked child of the test binary | The `Command` tests' forked children exit through libc `exit(3)` (faithful to the reference's `posix.exit`), so LSan scans them; with only the forking thread left, everything the parent's other test threads hold looks leaked. The scan of a fork is not a leak check of anything; the test binary itself is still checked in full. | N496 item 3: 4 reports, each consisting of the parent's heap (libtest's `main`, other tests' threads) |

## MemorySanitizer (G-MSAN)

MSan works on aarch64-unknown-linux-gnu with `-Zbuild-std` (verified on rvm, 2026-10-01: G-SAN-SELF's
uninitialized-read arm is RED, its clean control GREEN). It has no runtime suppressions file; a read of
memory written by uninstrumented code is reported, so its scope is an allow-list (`MSAN_SCOPE`) measured
by a probe of every workspace member (rvm, glibc 2.43, clang 22): 59 members in scope (one with named test skips), 2 excluded.

| id | what | kind | reason | evidence |
|---|---|---|---|---|
| M1 | `msan-stat-shim.h`, forced into the C of `fontconfig-sys` and `freetype-sys` only | tool workaround | rustc's MSan runtime does not mark `stat(2)`'s output initialized for a C caller on this glibc; clang's own runtime does. The shim calls the real `stat`/`lstat`/`fstat` and unpoisons the struct on success, which is what the interceptor does. Applied to two crates because it includes `<sys/stat.h>` ahead of the source's feature macros (applied everywhere, it hid `pthread_setname_np` from sentry-native). | Probe 1: `fontconfig-sys` RED in `FcFileIsDir` (`fcdir.c:47`, origin: the stack `statb`, filled by `stat`). Reproduced with a 2-line C function `stat("/")` + `S_ISDIR`: clean linked by `clang -fsanitize=memory`, RED linked by rustc. With the shim: 4 tests green. `fontconfig-sys`'s test also reaches freetype's `FT_Stream_Open` `fstat` (`builds/unix/ftsystem.c:279`, RED at `:300` without the shim), hence `freetype-sys`. |
| M2 | exclude `glslang-sys` | crate exclusion | Its test runs glslang's preprocessor, whose `TPpContext` holds a `std::istringstream` constructed and imbued (`std::locale::classic()`) inside the uninstrumented `libstdc++.so`. | Probe: `PpContext.cpp:98`, origin "allocation of 'ppContext' in the stack frame" of `InitializeSymbolTable`. Needs an MSan-instrumented C++ standard library. |
| M3 | `rustty-renderer` in scope with twelve named skips (`MSAN_SKIPS`, listed with reasons in `sanitizers.sh`): the modules or single tests that create an EGL/GL context or build shaders through glslang: `generic::linux_impl::tests`, `image::linux_impl::tests`, `shadertoy::`, `opengl::tests::`, `opengl::gl::tests::`, `opengl::gl::program::`, `opengl::opengl_impl::`, `opengl::shaders::`, `opengl::pipeline::`, `opengl::target::`, and two `opengl::gl::sys::tests` (`offscreen_egl_context_reaches_gl43_core`, `failed_prepare_context_restores_the_previous_table`) | test skips | Those tests drive EGL/OpenGL in the system's uninstrumented `libEGL`/Mesa, or translate shaders through glslang (M2). Fix round 1 replaced the first draft's whole-crate exclusion; fix round 2 replaced a module-wide `opengl::` skip with the list above, chosen by reading each test module (no GL context: `gl::glad`, `gl::texture`, `gl::buffer`, `gl::framebuffer`, `gl::draw`, `gl::primitives`, `gl::shader`, `gl::test_support`, `gpu_abi`, and six `gl::sys` tests run). Every filter must match a test (the gate lists the build's tests and is RED on a filter that matches none). | Probe: `eglQueryString` under `OffscreenContext::init` (`opengl/gl/sys.rs:195`), origin a `malloc` inside `/lib64/libEGL.so.1`; a `SAN_MSAN_DIAG=1` run (recover mode) also hit glslang's `PpContext.cpp:98` from the shader builds. With the twelve skips (rvm, 2026-10-02, commit 05478107 plus the refined skips): 81 passed, 73 skipped, 0 findings; the first narrowing (four skips) ran 52. |
| M4 | exclude `xtask` | crate exclusion | Its tests run nested `cargo`/`cmake` builds that inherit `RUSTFLAGS=-Zsanitizer=memory` without `-Zbuild-std`, which rustc rejects. It is the build driver (G-VALGRIND leaves it out too). | Probe: `error: mixing -Zsanitizer will cause an ABI mismatch in crate rustty_util` from `xtask/tests/cmake_lib_vt.rs`. |
| M5 | `msan-decl.h` (one prototype, no includes), forced into `sentry-sys`'s C | third-party build defect | sentry-native calls `__msan_unpoison` under `__has_feature(memory_sanitizer)` without including `<sanitizer/msan_interface.h>` (`src/sentry_unix_pageallocator.c:65`), so it does not compile under MSan. | rvm, 2026-10-01: `error: call to undeclared function '__msan_unpoison'`. Not ours; an upstream sentry-native fix would be the missing include. |

The other C++ `-sys` crates (`harfbuzz-sys`, `highway-sys`, `simdutf-sys`, `dcimgui-sys`, `breakpad-sys`,
`sentry-sys`, `spirv-cross-sys`) passed with their C++ instrumented, as did every crate that links them.
The GTK and Flatpak feature lanes are not in G-MSAN's scope (they link the uninstrumented GTK/GLib stack).
The C compilers are wrappers that add `-fsanitize=memory` only when the crate being built is instrumented
(`CARGO_CFG_SANITIZE`), because plain `CFLAGS` also reached `rustty-cli`'s `zlib-sys` build-dependency
and its build script failed to link (`undefined reference to __msan_init`).

## HWAddressSanitizer (G-HWASAN, P7-W16b)

HWASan exists for aarch64 Linux only (rustc: "hwaddress sanitizer is not supported for this target" for
`aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`), so G-HWASAN runs on rvm and is BLOCKED elsewhere.
It needs the aarch64 tagged-address ABI: rvm's kernel (6.19, Fedora 44) has
`CONFIG_ARM64_TAGGED_ADDR_ABI=y`, `abi.tagged_addr_disabled=0`, and a probe under HWASan read
`PR_GET_TAGGED_ADDR_CTRL = 1` (the runtime turns it on). Probes on rvm, 2026-10-02 (nightly 2026-07-21,
LLVM 22.1.8): a heap read 8 bytes and 1 byte past a 16-byte block, and a use-after-free, are each reported
(`tag-mismatch`, Cause `heap-buffer-overflow` / `use-after-free`); a clean test passes.

| id | what | kind | reason | evidence |
|---|---|---|---|---|
| H0 | `-Zbuild-std -Ctarget-feature=+tagged-globals,-outline-atomics -Clink-arg=-fuse-ld=lld` | lane configuration | Each is needed: without `-Zbuild-std` rustc rejects the mix (`-Zsanitizer=hwaddress` is "incompatible with `-Zsanitizer` being unset in dependency `core`"); without `-outline-atomics`, compiler_builtins' outline-atomics asm takes `adrp` of a tagged global and the link fails (`relocation truncated to fit: R_AARCH64_ADR_PREL_PG_HI21 against ... HAVE_LSE_ATOMICS`, both ld.bfd and lld); GNU ld 2.46 links tagged globals into a binary that dies with SIGILL before `main` (an invalid instruction, `.inst 0xb2ea5fe8`, in std's `args::really_init`), so the link uses lld. `+tagged-globals` is what clang turns on itself for `-fsanitize=hwaddress`; rustc warns that it is an "unknown and unstable feature". | probe crate on rvm, each failure reproduced one flag at a time |
| H1 | exclude `xtask` | crate exclusion | Its tests run nested cargo builds that inherit `RUSTFLAGS=-Zsanitizer=hwaddress` without `-Zbuild-std`, which rustc rejects. The same reason as M4. | first G-HWASAN run (rvm, 2026-10-02): 38 nested builds failed with `mixing -Zsanitizer will cause an ABI mismatch in crate rustty_util`, so 2 tests failed (`install_layout_is_versioned_like_the_reference`, `lib_version_string_versions_the_library`); N504 |
| H2 | the C-ABI smoke (`rustty-apprt-c` `c_abi_smoke_compile_link_run`) links its C program with `clang -fsanitize=hwaddress -fuse-ld=lld` under `cfg(rustty_hwasan)` | test-side configuration | The instrumented cdylib leaves `__hwasan_*` to the executable (as for ASan, N496 item 4). clang 22's HWASan runtime matches rustc's, so the test runs; under MSan it does not (the smoke is ignored there). | N504. First run: `undefined reference to __hwasan_init` (and 8 more) linking the smoke; with the flags it passes (rvm, 32 tests in the crate) |

The C and C++ we compile through `cc` are instrumented with `clang -fsanitize=hwaddress -mno-outline-atomics`
through the same kind of wrapper as G-MSAN's (only when the crate being built is instrumented): checked on
`oniguruma-sys`, whose `libonig.a` has 438 `__hwasan_*` references and whose `onig_search` calls
`__hwasan_check_*`. Uninstrumented code (system libraries, libstdc++, Mesa, GTK) needs nothing: it
receives tagged pointers from HWASan's `malloc` and the aarch64 Top-Byte-Ignore makes its loads and stores
work, so it runs unchecked. HWASan replaces glibc's allocator through the same `malloc`/`free`
interceptors ASan uses; the first run found no allocator or tagged-pointer breakage anywhere, including
EGL/Mesa (`rustty-renderer`, 154 tests) and the GTK apprt lane (174 tests). It has no runtime suppression;
`detect_leaks=0` and `allocator_may_return_null=1` are its options (table above).

**What it adds over G-ASAN.** HWASan gives every heap block (and stack slot and global) a random 8-bit
tag, stored in the pointer's top byte and in a 1/16 shadow, and checks it on every access. ASan surrounds
blocks with redzones and quarantines freed memory. So:

- **an access that lands inside another live block**, past the redzones, is invisible to ASan and a
  `tag-mismatch` under HWASan. Probe on rvm: one heap block indexed straight into a neighbour's interior,
  ASan `test result: ok`, HWASan `Cause: heap-buffer-overflow`. G-SAN-SELF's A4 wild-index arm keeps it.
- **a use-after-free after the block is reused** is caught however long ago the free was (the block is
  retagged on reuse), where ASan relies on its 256 MiB quarantine. In the probe (8 M frees of the same size
  before reuse) both caught it.
- **lower memory overhead**: no redzones or quarantine, and the shadow is 1/16 of memory instead of ASan's
  1/8.
- **the vendored C/C++ is instrumented**, which G-ASAN's is not (Known limits below).
- **but detection is probabilistic**: two blocks share a tag with probability about 1/256, so a single run
  can miss a given bad access (G-SAN-SELF's wild-index arm retags until the two tags differ). ASan's
  redzone and quarantine checks are deterministic.

**Runs (rvm, 2026-10-02, `--per-crate-target`).** First run: 77 min, 9,955 tests GREEN over 61 members
and the three feature lanes, zero HWASan reports; RED on H1, H2 and a cross-check parser defect (N504),
all fixed; the cross-check over its logs then read 468 Miri-excepted, 412 ran and passed, 56 expected
absent, 0 problems. A second run on the fixed tree stopped at the Mac's 15 GB disk floor after 45 of 63
specs (44 GREEN, 2,029 tests, 0 RED); a complete GREEN run is still owed (N504).

So G-HWASAN is a complement to G-ASAN, not a replacement: G-ASAN stays the everyday lane (macOS and Linux,
deterministic), and G-HWASAN adds the wild-pointer class and instrumented C on rvm.

## Crate exclusions and empty lanes

No workspace member is excluded from G-ASAN or G-LSAN; G-HWASAN excludes `xtask` (H1). The G-LSAN matrix behind fix round 1's numbers ran on commit 7ddf62ffd (10,111 tests; its cross-check RED only for the then-missing Linux expected-absent rows); the full G-LSAN gate re-ran GREEN on 05478107 (10,111 tests, 18 min, cross-check 412 ran / 56 expected absent / 0 problems). `NO_TESTS_ON` in `sanitizers.sh` lists the members
that have no test on a host OS by construction (they are still built under the sanitizer):

| crate | OS | reason |
|---|---|---|
| `gettext-sys` | Linux | libintl is part of glibc there; the crate compiles no C and every test is `#[cfg(all(test, target_os = "macos"))]`. |
| `rustty-wasm` | all | its one native test (`tests/artifact_shape.rs`) is `#[ignore]`d: it inspects the wasm32 artifact the `--webgl` lane builds; the exports are `cfg(target_arch = "wasm32")`. |

## Known limits (open)

- **G-ASAN and G-LSAN compile the vendored C/C++ without `-fsanitize=address`.** ASan still sees that
  code's heap through its `malloc`/`free`/`mem*`/`str*` interceptors (so a C buffer freed twice, or leaked,
  is reported), but not the C code's own loads and stores (an out-of-bounds read inside harfbuzz is not).
  The design (§B) asks for the C instrumented too. G-MSAN already does it through compiler wrappers; doing
  the same for ASan needs a clang whose ASan ABI matches rustc's runtime, which holds for Fedora's clang 22
  but is unverified for Apple clang on macOS. Not done in P7-W14. On aarch64 Linux, G-HWASAN (P7-W16b)
  does instrument that C/C++, so its loads and stores are checked there, by tag rather than redzone.
- **G-HWASAN runs on rvm only.** No hosted job runs it (GitHub's `sanitizers` job is x86_64, where
  HWASan does not exist); an `ubuntu-24.04-arm` leg would be its CI copy (V31).
- MSan's scope excludes the two crates above and skips rustty-renderer's EGL/GL/glslang test modules until an instrumented C++ standard library and EGL are
  available (an x86_64 CI image could provide an MSan-built libc++).

## Miri-excepted tests expected absent from a sanitizer pass

G-ASAN, G-LSAN and G-HWASAN check that every Miri-excepted test ran and passed under them
(`san_miri_crosscheck.py`). The ones that cannot run on a host OS (cfg-gated off it, or natively
`#[ignore]`d there) are listed, per OS and with reasons, in `.port/tools/gates/san_expected_absent.tsv`:
9 on macOS (7 Windows-only kitty path tests, one Linux-only `/proc` test, one release-only benchmark
test) and 56 on Linux (the `rustty-xev` kqueue backend, the `rustty-os` macOS modules, the non-pipe2
fallback, two termio `setup_fd` tests, and the same Windows and benchmark rows). A row whose tests
ran, or that matches nothing, is RED.
