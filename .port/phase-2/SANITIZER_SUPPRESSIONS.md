# Sanitizer suppressions and exclusions (G-ASAN, G-LSAN, G-MSAN)

The sanitizer gates (`.port/tools/gates/sanitizers.sh --gate asan|lsan|msan`,
registered in `.port/tools/ci.sh`; design:
`docs/superpowers/specs/2026-10-01-memory-safety-lanes-design.md` §B) run every
workspace crate's tests under AddressSanitizer, ASan + LeakSanitizer and
MemorySanitizer. This file is the register of everything those gates do NOT
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

- G-LSAN reads `.port/phase-2/lsan.supp` (Linux) or `.port/phase-2/lsan-darwin.supp`
  (macOS) through `LSAN_OPTIONS=suppressions=…:print_suppressions=1`, so every
  suppressed leak is listed in the run's log. Each line in either file has a WHY
  comment above it (D6 rule c, `d6_cfg_mapping_audit.py`) pointing at its row here.
- Crate exclusions are the `ASAN_EXCLUDES`, `LSAN_EXCLUDES` and `MSAN_EXCLUDES`
  arrays in `sanitizers.sh`, each `"<crate>|<reason>"`. An entry naming a crate
  that is no longer a workspace member is RED. G-MSAN's scope is an allow-list:
  every member must be in `MSAN_SCOPE` or `MSAN_EXCLUDES`.

## LeakSanitizer suppressions (`lsan.supp`)

| id | pattern | library | reason | evidence |
|---|---|---|---|---|
| S1 | `leak:libEGL_mesa.so` | Mesa EGL (system, uninstrumented) | `eglInitialize` on the surfaceless platform allocates per-process display state inside Mesa that is never freed. The port, like Ghostty (`OpenGL.zig:113-115`, "Do not destroy the EGL display here as it is shared across the entire process"; DECISIONS B10), never terminates the shared display. | G-LSAN, rvm, 2026-10-01, `rustty-renderer`: 28 leak reports, every one with its first non-allocator frame in `/lib64/libEGL_mesa.so.0` (5) or `/lib64/libgallium-26.1.4.so` (23), the Rust frames below it ending at `OffscreenContext::init` (`crates/rustty-renderer/src/opengl/gl/sys.rs:214`, the `egl.initialize(display)` call), reached from `generic::linux_impl::tests::golden_renderer`. |
| S2 | `leak:libgallium` | Mesa gallium/llvmpipe driver (system) | Loaded by the same `eglInitialize`; its screen and shader-cache allocations live for the process. | as S1. The final G-LSAN run (rvm, 2026-10-02) lists what both matched (`print_suppressions=1`): S1 139 leaks, 28,395 bytes; S2 940 leaks, 132,019 bytes; all in `rustty-renderer`, nothing anywhere else. |

### macOS (Darwin) entries (`lsan-darwin.supp`, P7-W16a)

G-LSAN runs on macOS since P7-W16a (see "Platform support" below). LeakSanitizer on Darwin takes its
roots from thread stacks and registers, the writable sections of loaded modules and three VM-tagged
regions (compiler-rt `lib/lsan/lsan_common_mac.cpp`, `ProcessGlobalRegions` and
`ProcessPlatformSpecificAllocations`). Per-thread data that libobjc and CoreFoundation reach only
through pthread thread-specific data, and SkyLight state held by its dispatch sources, are not
roots there, so they are reported while their owners are alive. The arbiter is Apple's `leaks`
(`leaks --atExit`, with `MallocStackLogging=1`), which knows those roots: on the natively built test
binaries that reported them (`rustty_apprt` unit tests, `dcimgui-sys`'s `metal_offscreen` after the
UD376 fix) it reports **0 leaks**. Each pattern names the allocating system function, so a block whose
allocation runs through our code (or through `_objc_rootAllocWithZone` below our call) is still
reported; the Dear ImGui Metal backend's leaks (UD376) were found that way, and Apple's `leaks`
agreed (54 leaks, 10,416 bytes, before the fix).

| id | pattern | library | reason | evidence |
|---|---|---|---|---|
| S3 | `leak:_fetchInitializingClassList` | libobjc | the per-thread list of classes being `+initialize`d, owned by the thread's objc TSD | G-LSAN macOS survey run (P7-W16a, no Darwin suppressions): every crate that touches Objective-C (`dcimgui-sys`, `rustty-apprt` lib and `global_init_resyncs_environ`, `rustty`'s CLI), frames `_fetchInitializingClassList` / `_setThisThreadIsInitializingClass` / `initializeNonMetaClass`, all in `libobjc.A.dylib`; `leaks`: 0 |
| S4 | `leak:fetch_cache` | libobjc | the per-thread `@synchronized` lock cache (`objc_sync_enter` → `id2data` → `fetch_cache`), owned by objc TSD | as S3 |
| S5 | `leak:__CFTSDGetTable` | CoreFoundation | CF's per-thread TSD table, created on a thread's first CF call and freed by CF's TSD destructor at thread exit | `dcimgui-sys` (a libdispatch worker thread under Metal's telemetry block), `rustty-apprt` lib; `leaks`: 0 |
| S6 | `leak:AutoreleasePoolPage::autoreleaseNoPage` | libobjc | the autorelease-pool PAGE a thread gets when it autoreleases with no pool; drained and freed by libobjc's TSD destructor. The autoreleased objects themselves are allocated elsewhere and are not matched | `dcimgui-sys` `metal_offscreen` (Foundation `bytesInEncoding`); `leaks`: 0 |
| S7 | `leak:RunElsewhere::create_source` | SkyLight | a `mach_msg` receive buffer allocated inside SkyLight's `RunElsewhere` dispatch-source handler, held by that source | `rustty-apprt` lib: `mach_msg_overwrite` ← `mach_msg` ← `(anonymous namespace)::RunElsewhere::create_source()` block (`SkyLight`) on a libdispatch worker; `leaks`: 0 |
| S8 | `leak:update_devices_from_data_internal` | SkyLight | SkyLight's display-device cache (CFNumbers and CFData decoded by `CGSPropertyListCreateWithSerializedBytes`), built when the display configuration is first read, held for the process | `rustty-apprt` lib: `CFNumberCreate` ← `CGSPropertyListCreateWithSerializedBytes` ← `update_devices_from_data_internal` ← `devices_connection_change_handler` (`SkyLight`); `leaks`: 0 |
| S9 | `leak:libFontRegistry.dylib` | libFontRegistry (CoreText) | the process-wide font registry created once under `pthread_once` (`TGlobalFontRegistry::CreateRegistry`) caches the font server's rendezvous reply (NSXPC-decoded dictionaries, CFURLs, strings) for the life of the process | `rustty-apprt-c` (lib tests, the C smoke binary, `macos_window_host`): `CTFontCreateWithName` → `XTCopyFontWithName` → `TGlobalFontRegistry::CreateRegistry` → `RendezvousWithServer` → `TRequestCache::AddResultForRequest` / NSXPC decode, within the 30 recorded frames. Reproduced with no Rustty code: a probe that creates and releases one CTFont (on a thread and on the main thread) reports exactly this set (286 allocations, 21,472 bytes) under G-LSAN, and Apple `leaks` reports 0 for the same sequence in C. Negative control: the probe that does NOT release its CTFont is still RED (`TCFRef<CTFont>`, 2,432 bytes) with S9/S10 in place |
| S10 | `leak:TDescriptorSource::CopyDescriptorUncached` | CoreText | CoreText's font-descriptor cache, filled by `TDescriptorSource::CopyDescriptorUncached` under `CTFontCreateWithName`; an unreleased CTFont is reported through `TCFBase_NEW<CTFont>`, which this does not match | as S9 (96 bytes, one descriptor) |
| S11 | `leak:dyld::ThreadLocalVariables::instantiateVariable` | dyld | the thread-local-variable block dyld allocates on a thread's first TLV access and frees from its TSD destructor at thread exit; reported for a test child that calls `exit(3)` from its test thread, which is still alive at the scan | `rustty-apprt-c` `cli_try_action_version_runs_and_exits` child (400 bytes, `_tlv_get_addr` ← std `thread_start`), `rustty-cli` picker children (192 bytes) |
| S12 | `leak:dyld::ThreadLocalVariables::addTermFunc` | dyld | dyld's per-thread list of thread-local destructors (`_tlv_atexit`), run and freed at thread exit; same situation as S11 | `rustty-cli` `list_themes::picker_tests` children (128 bytes each, ← std `thread_start`) |
| S13 | `leak:TCGFontCache::CopyFont` | CoreText / CoreGraphics | CoreText's CGFont cache (`TCGFontCache::CopyFont` → `CGFontCreateWithPathAndName`), filled when a font file's tables are first read, with its data provider and file reference beneath; an unreleased CTFont is reported through `TCFBase_NEW<CTFont>`, which this does not match | `rustty-font-shaper` lib tests (`discovery::coretext_impl::Score::score` → `CTFontCreateWithFontDescriptor`); Apple `leaks` on the natively built test binary: 0 leaks (141 tests) |
| S14 | `leak:CA::Transaction::create` | QuartzCore (CoreAnimation) | CoreAnimation's per-thread transaction, created on a thread's first layer change and kept in CA's thread-specific data until thread exit | `rustty-apprt-c` `macos_window_host` (8,192 bytes); Apple `leaks` on the same binary built natively lists no `CA::Transaction` (its 83 leaks are N501's) |

## Sanitizer runtime options set by the lanes

Not suppressions, but they change what a lane does, so each is recorded with its reason (also next to
the option in `sanitizers.sh`).

| option | lanes | reason | evidence |
|---|---|---|---|
| `detect_leaks=0` | G-ASAN | G-LSAN (the same ASan build with `detect_leaks=1`, on macOS and Linux) is the leak lane, so G-ASAN's verdict stays memory errors. (Until P7-W16a this row said Apple's ASan has no leak checker: true of Apple clang's runtime, not of rustc's, which G-LSAN uses.) | design §B; P7-W16a |
| `-Zexport-executable-symbols` | G-LSAN on macOS | The macOS ASan runtime is a dylib that looks the user hooks (`__lsan_is_turned_off`, `__lsan_default_suppressions`) up dynamically (`nm -m librustc-nightly_rt.asan.dylib`: "weak external `___lsan_is_turned_off` (dynamically looked up)"), and rustc exports nothing from an executable by default, so `rustty-termio`'s fork-child hook (below) was never seen. Not suppressive: it only makes the documented hook visible. On Linux the runtime is static and the hook resolves at link time. | P7-W16a: a probe whose hook always returns 1 reported its leak without the flag and not with it; G-SAN-SELF's "opt-out hook" arm is RED without the flag (mutation run) |
| `log_path=<private temp dir>/rt` (runtime output to per-process files, appended to the crate log after the run) | G-LSAN on macOS | With `lsan-darwin.supp`, `print_suppressions=1` prints its table on the stderr of every process in which a suppression matched, including CLI children whose stderr a test asserts on (`rustty`'s `cli_log_facade`: `+show-config` must print nothing). Not suppressive: every report and table still reaches the crate log the classifier reads, and a child that leaks still fails its test (exit 23). Linux keeps stderr (no Linux suppression matches in a child). | P7-W16a: run 2's `cli_log_facade` failure, stderr = the S3/S4 table only; with `log_path` the crate is GREEN (17 tests) and both tables are in its log; the probe's leak is still RED |
| `allocator_may_return_null=1` | G-ASAN, G-LSAN, G-MSAN | ASan's malloc aborts on a request above its 1 TiB maximum, where the system malloc returns NULL; the port's out-of-memory tests rely on that NULL (`try_reserve` reporting failure, the port of Zig's `error.OutOfMemory` paths). It does turn off three ASan reports: allocation-size-too-big (a request over the allocator's maximum), calloc-overflow (count x size overflowing) and out-of-memory (the allocator failing): each now returns NULL to the caller instead of aborting with a report. Out-of-bounds, use-after-free, double-free, leak and uninitialized-read detection are unaffected, and a NULL the code does not handle still crashes visibly. | NEW-FINDINGS N496 item 5 |
| `RUST_MIN_STACK=8388608` | all three | Instrumented frames are 2-3x their native size; a test that checks a recursion bound fits the default 2 MiB stack overflows under ASan with no memory error. Unbounded recursion still overflows 8 MiB; the native bound is checked by `cargo test --workspace`. | N496 item 6 |
| line-tables-only debuginfo | all three | Full debuginfo made the GTK lane's instrumented test links exhaust the 10 GB VM (ld SIGBUS). Reports keep every function, file and line. | N496 item 7 |

## Test-binary hooks

| where | what | reason | evidence |
|---|---|---|---|
| `crates/rustty-termio/src/command.rs` tests module | defines `__lsan_is_turned_off`, non-zero only in a forked child of the test binary (Linux and, since P7-W16a, macOS) | The `Command` tests' forked children exit through libc `exit(3)` (faithful to the reference's `posix.exit`), so LSan scans them; with only the forking thread left, everything the parent's other test threads hold looks leaked. The scan of a fork is not a leak check of anything; the test binary itself is still checked in full. | N496 item 3: 4 reports, each consisting of the parent's heap (libtest's `main`, other tests' threads) |

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

## Crate exclusions and empty lanes

No workspace member is excluded from G-ASAN or G-LSAN. The G-LSAN matrix behind fix round 1's numbers ran on commit 7ddf62ffd (10,111 tests; its cross-check RED only for the then-missing Linux expected-absent rows); the full G-LSAN gate re-ran GREEN on 05478107 (10,111 tests, 18 min, cross-check 412 ran / 56 expected absent / 0 problems). `NO_TESTS_ON` in `sanitizers.sh` lists the members
that have no test on a host OS by construction (they are still built under the sanitizer):

| crate | OS | reason |
|---|---|---|
| `gettext-sys` | Linux | libintl is part of glibc there; the crate compiles no C and every test is `#[cfg(all(test, target_os = "macos"))]`. |
| `rustty-wasm` | all | its one native test (`tests/artifact_shape.rs`) is `#[ignore]`d: it inspects the wasm32 artifact the `--webgl` lane builds; the exports are `cfg(target_arch = "wasm32")`. |

## Platform support on macOS (P7-W16a, 2026-10-02)

Measured on the aarch64-apple-darwin dev box (macOS 26.5, nightly 2026-07-22 with LLVM 22.1.8, Apple
clang 21.0.0, Homebrew LLVM 21), with a two-test probe crate (64 leaked boxes; a clean control).

| sanitizer | macOS verdict | evidence |
|---|---|---|
| LeakSanitizer | **runs** as G-LSAN: ASan with `detect_leaks=1` | The standalone `-Zsanitizer=leak` fails to COMPILE: "leak sanitizer is not supported for this target" (rustc's `aarch64_apple_darwin.rs` lists ADDRESS, CFI, THREAD, REALTIME, with "FIXME: The leak sanitizer currently fails the tests, see #88132", where the standalone LSan UI test hung on arm64 macOS; the unstable book lists LSan for `x86_64-apple-darwin` only). rustc's ASan runtime (its own compiler-rt, where `lsan_common.h` sets `CAN_SANITIZE_LEAKS` for `SANITIZER_APPLE` on `__aarch64__`, and `sanitizer_flags.inc` only defaults `detect_leaks` to `!SANITIZER_APPLE`) reports the probe's leak ("LeakSanitizer: detected memory leaks … 16384 byte(s) leaked in 64 allocation(s)", exit 1) and passes the clean test. Apple clang's own ASan runtime aborts at RUNTIME with "AddressSanitizer: detect_leaks is not supported on this platform" (exit 134), and Apple clang rejects `-fsanitize=leak` at compile time; that runtime is what earlier notes meant by "Apple's ASan has no leak checker". Homebrew LLVM 21: its ASan deadlocks in its own init on macOS 26 (`AsanInitInternal` → `get_dyld_hdr` → `dyld_shared_cache_iterate_text_swift` → `_Block_copy` → malloc), and its standalone `-fsanitize=leak` binary spins in `CheckForLeaksCallback` under `StopTheWorld`; not usable. |
| MemorySanitizer | **unsupported**; Linux-only G-MSAN is the lane | compiler-rt builds MSan for `Linux|FreeBSD|NetBSD` only (`cmake/config-ix.cmake`, `COMPILER_RT_HAS_MSAN`); rustc: "memory sanitizer is not supported for this target"; Apple clang: "unsupported option '-fsanitize=memory'". Valgrind does not run on Apple Silicon (not installed, no arm64-darwin port). The macOS uninitialized-read coverage is Miri (G6/G6c: uninit reads are UB it reports) for pure Rust, plus G-MSAN on Linux for the C/C++ the port compiles; nothing comparable for C on macOS. |
| ThreadSanitizer | supported (`-Zsanitizer=thread` with `-Zbuild-std`; rustc's target spec and the unstable book list `aarch64-apple-darwin`; compiler-rt `COMPILER_RT_HAS_TSAN` includes Darwin) | not yet a gate (G-TSAN, design §B) |
| HWAddressSanitizer | **unsupported** | compiler-rt builds it for `Linux|Android|Fuchsia` only (`COMPILER_RT_HAS_HWASAN`); rustc: "hwaddress sanitizer is not supported for this target"; Apple clang: "unsupported option '-fsanitize=hwaddress'". Handled on Linux by a separate task. |

Leak-checking alternatives evaluated (P7-W16a):

- **Apple `leaks --atExit -- <test binary>`** works on native test binaries: RED on the probe's leak
  (64 ROOT LEAKs, exit 1), GREEN on the clean test (exit 0); quiet on `rustty-font` (89 tests) and the
  `rustty-apprt` unit tests (0 leaks); about 1.3x native wall time (`rustty-font`: 6.6 s native, 8.5 s
  under `leaks`, 10.0 s with `MallocStackLogging=1`). It scans only the main process at exit (not forked
  children) and needs `MallocStackLogging` for allocation stacks. Used here as the ARBITER for the S3-S14
  suppressions, not as a separate gate: G-LSAN already gives the same detection, stacks, the
  suppression file, the Miri-exception cross-check and the existing self-tests.
- **Homebrew LLVM compiler-rt**: not usable on macOS 26 (above).
- **A tracking allocator**: PORT_RULES R6 removed that approach in favour of Miri plus the sanitizers;
  with a working LSan on macOS it is not needed.

## Known limits (open)

- **G-ASAN and G-LSAN compile the vendored C/C++ without `-fsanitize=address`.** ASan still sees that
  code's heap through its `malloc`/`free`/`mem*`/`str*` interceptors (so a C buffer freed twice, or leaked,
  is reported), but not the C code's own loads and stores (an out-of-bounds read inside harfbuzz is not).
  The design (§B) asks for the C instrumented too. G-MSAN already does it through compiler wrappers; doing
  the same for ASan needs a clang whose ASan ABI matches rustc's runtime, which holds for Fedora's clang 22
  but is unverified for Apple clang on macOS. Not done in P7-W14.
- MSan's scope excludes the two crates above and skips rustty-renderer's EGL/GL/glslang test modules until an instrumented C++ standard library and EGL are
  available (an x86_64 CI image could provide an MSan-built libc++).

## Miri-excepted tests expected absent from a sanitizer pass

G-ASAN and G-LSAN check that every Miri-excepted test ran and passed under them
(`san_miri_crosscheck.py`). The ones that cannot run on a host OS (cfg-gated off it, or natively
`#[ignore]`d there) are listed, per OS and with reasons, in `.port/tools/gates/san_expected_absent.tsv`:
9 on macOS (7 Windows-only kitty path tests, one Linux-only `/proc` test, one release-only benchmark
test) and 56 on Linux (the `rustty-xev` kqueue backend, the `rustty-os` macOS modules, the non-pipe2
fallback, two termio `setup_fd` tests, and the same Windows and benchmark rows). A row whose tests
ran, or that matches nothing, is RED.
