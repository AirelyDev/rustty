# Sanitizer suppressions and exclusions (G-ASAN, G-LSAN, G-MSAN, G-HWASAN, G-TSAN)

The sanitizer gates (`.port/tools/gates/sanitizers.sh --gate asan|lsan|msan|hwasan|tsan`,
registered in `.port/tools/ci.sh`; design:
`docs/superpowers/specs/2026-10-01-memory-safety-lanes-design.md` §B) run every
workspace crate's tests under AddressSanitizer, ASan + LeakSanitizer,
MemorySanitizer and (aarch64 Linux, P7-W16b) HWAddressSanitizer, and the threaded crates under
ThreadSanitizer (P7-W17). This file is the register of everything those gates do NOT
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
  suppressed leak is listed in the run's log. G-LSAN-GTK (P7-W24) reads `lsan.supp` followed by
  `.port/phase-2/lsan-gtk.supp`, concatenated (LSan takes one file). Each line in any of them has a WHY
  comment above it (D6 rule c, `d6_cfg_mapping_audit.py`) pointing at its row here.
- G-TSAN reads `.port/phase-2/tsan.supp` through `TSAN_OPTIONS=suppressions=…:print_suppressions=1`
  (empty so far; ThreadSanitizer section, T3).
- Crate exclusions are the `ASAN_EXCLUDES`, `LSAN_EXCLUDES`, `HWASAN_EXCLUDES` and `MSAN_EXCLUDES`
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

**Standing negative controls (fix round 1).** Several entries name system functions that allocate on a
caller's behalf (S10, S13, S17, S18), so G-SAN-SELF has Darwin arms that leak, through the same APIs,
the objects Rustty could leak, and requires each to stay RED under the real `lsan-darwin.supp`: a CTFont
(`CTFontCreateWithName`), a CTFontDescriptor created (`CTFontDescriptorCreateWithNameAndSize`) and one
copied from a font (`CTFontCopyFontDescriptor`), a character set derived from a font's (`CFCharacterSetCreateMutableCopy`, fix round 2), a CGFont (`CGFontCreateWithFontName`) and an
`MTLCopyAllDevices` array; a CoreText create-and-release control must be GREEN. One shape is beyond any
reachability-based checker and so is not an arm: an extra retain on the CACHED CGFont that
`CTFontCopyGraphicsFont` returns stays reachable from CoreText's cache, and was GREEN with S13 removed too.

| id | pattern | library | reason | evidence |
|---|---|---|---|---|
| S3 | `leak:_fetchInitializingClassList` | libobjc | the per-thread list of classes being `+initialize`d, owned by the thread's objc TSD | G-LSAN macOS survey run (P7-W16a, no Darwin suppressions): every crate that touches Objective-C (`dcimgui-sys`, `rustty-apprt` lib and `global_init_resyncs_environ`, `rustty`'s CLI), frames `_fetchInitializingClassList` / `_setThisThreadIsInitializingClass` / `initializeNonMetaClass`, all in `libobjc.A.dylib`; `leaks`: 0 |
| S4 | `leak:^fetch_cache(bool)$` (anchored, fix round 1) | libobjc | the per-thread `@synchronized` lock cache (`objc_sync_enter` → `id2data` → `fetch_cache`), owned by objc TSD | as S3 |
| S5 | `leak:__CFTSDGetTable` | CoreFoundation | CF's per-thread TSD table, created on a thread's first CF call and freed by CF's TSD destructor at thread exit | `dcimgui-sys` (a libdispatch worker thread under Metal's telemetry block), `rustty-apprt` lib; `leaks`: 0 |
| S6 | `leak:AutoreleasePoolPage::autoreleaseNoPage` | libobjc | the autorelease-pool PAGE a thread gets when it autoreleases with no pool; drained and freed by libobjc's TSD destructor. The autoreleased objects themselves are allocated elsewhere and are not matched | `dcimgui-sys` `metal_offscreen` (Foundation `bytesInEncoding`); `leaks`: 0 |
| S7 | `leak:RunElsewhere::create_source` | SkyLight | a `mach_msg` receive buffer allocated inside SkyLight's `RunElsewhere` dispatch-source handler, held by that source | `rustty-apprt` lib: `mach_msg_overwrite` ← `mach_msg` ← `(anonymous namespace)::RunElsewhere::create_source()` block (`SkyLight`) on a libdispatch worker; `leaks`: 0 |
| S8 | `leak:update_devices_from_data_internal` | SkyLight | SkyLight's display-device cache (CFNumbers and CFData decoded by `CGSPropertyListCreateWithSerializedBytes`), built when the display configuration is first read, held for the process | `rustty-apprt` lib: `CFNumberCreate` ← `CGSPropertyListCreateWithSerializedBytes` ← `update_devices_from_data_internal` ← `devices_connection_change_handler` (`SkyLight`); `leaks`: 0 |
| S9 | `leak:TGlobalFontRegistry::CreateRegistry`, `leak:TRequestCache::AddResultForRequest` and `leak:XTCopyPropertiesForFont` (fix rounds 1-2: the observed registry frames, no longer the whole library; the third matches both the per-font property query `XTCopyPropertiesForFont` and the font-matching request `XTCopyPropertiesForFontsMatchingRequest`, each seen intermittently in `rustty-font-shaper` once the library pattern was narrowed. A character set Rustty copies with `CTFontCopyCharacterSet` and leaks is not reported even without the entry, 0 of 8 runs, as CoreText keeps it reachable; a set derived from it, `CFCharacterSetCreateMutableCopy`, stays RED with it) | libFontRegistry (CoreText) | the process-wide font registry created once under `pthread_once` (`TGlobalFontRegistry::CreateRegistry`) caches the font server's rendezvous reply (NSXPC-decoded dictionaries, CFURLs, strings) for the life of the process | `rustty-apprt-c` (lib tests, the C smoke binary, `macos_window_host`): `CTFontCreateWithName` → `XTCopyFontWithName` → `TGlobalFontRegistry::CreateRegistry` → `RendezvousWithServer` → `TRequestCache::AddResultForRequest` / NSXPC decode, within the 30 recorded frames. Reproduced with no Rustty code: a probe that creates and releases one CTFont (on a thread and on the main thread) reports exactly this set (286 allocations, 21,472 bytes) under G-LSAN, and Apple `leaks` reports 0 for the same sequence in C. Negative control: the probe that does NOT release its CTFont is still RED (`TCFRef<CTFont>`, 2,432 bytes) with S9/S10 in place. With the two narrowed patterns the create-and-release probe stays GREEN (88 + 136 allocations matched) |
| S10 | `leak:TDescriptorSource::CopyDescriptorUncached` | CoreText | CoreText's font-descriptor cache, filled by `TDescriptorSource::CopyDescriptorUncached` under `CTFontCreateWithName`; an unreleased CTFont is reported through `TCFBase_NEW<CTFont>`, which this does not match | as S9 (96 bytes, one descriptor) |
| S11 | `leak:dyld::ThreadLocalVariables::instantiateVariable` | dyld | the thread-local-variable block dyld allocates on a thread's first TLV access and frees from its TSD destructor at thread exit; reported for a test child that calls `exit(3)` from its test thread, which is still alive at the scan | `rustty-apprt-c` `cli_try_action_version_runs_and_exits` child (400 bytes, `_tlv_get_addr` ← std `thread_start`), `rustty-cli` picker children (192 bytes) |
| S12 | `leak:dyld::ThreadLocalVariables::addTermFunc` | dyld | dyld's per-thread list of thread-local destructors (`_tlv_atexit`), run and freed at thread exit; same situation as S11 | `rustty-cli` `list_themes::picker_tests` children (128 bytes each, ← std `thread_start`) |
| S13 | `leak:TCGFontCache::CopyFont` | CoreText / CoreGraphics | CoreText's CGFont cache (`TCGFontCache::CopyFont` → `CGFontCreateWithPathAndName`), filled when a font file's tables are first read, with its data provider and file reference beneath; an unreleased CTFont is reported through `TCFBase_NEW<CTFont>`, which this does not match | `rustty-font-shaper` lib tests (`discovery::coretext_impl::Score::score` → `CTFontCreateWithFontDescriptor`); Apple `leaks` on the natively built test binary: 0 leaks (141 tests) |
| S14 | `leak:CA::Transaction::create` | QuartzCore (CoreAnimation) | CoreAnimation's per-thread transaction, created on a thread's first layer change and kept in CA's thread-specific data until thread exit | `rustty-apprt-c` `macos_window_host` (8,192 bytes); Apple `leaks` on the same binary built natively lists no `CA::Transaction` (its 83 leaks are N502's) |
| S15 | `leak:CGColorSyncTransformCacheGetRetained` | CoreGraphics / ColorSync | the ColorSync transform cache (libcache-backed), filled by the first conversion between two colour spaces and held for the process | `rustty-renderer` `preedit_glyph_uses_font_thicken` (`ColorSyncTransformCreate` and `colorsync_transform_data_key_retain` ← `CGColorSyncTransformCacheGetRetained` ← `CGCMSConverterCreate`); Apple `leaks` on the natively built renderer tests: 0 leaks (140 tests) |
| S16 | `leak:CGCMSConverterCreateCachedCGvImageConverter` | CoreGraphics | the cached vImage converter kept with an S15 transform | as S15 (824 bytes) |
| S17 | `leak:-[_MTLCommandQueue initWithDevice:descriptor:]` | Metal / libdispatch | the private dispatch queue a Metal command queue creates inside its `init`, disposed of asynchronously by libdispatch after the command queue is released; a process exiting right after freeing its renderer can still hold it at the scan (timing-dependent). Allocations of the command queue object itself happen in `newCommandQueue`'s alloc, outside this frame, so a leaked command queue stays reported | `rustty-apprt-c` `macos_window_host` `init_error_path_joins_both_threads_before_freeing_their_state` child (128 bytes, `_os_object_alloc_realized` ← `_dispatch_lane_create_with_target` ← `-[_MTLCommandQueue initWithDevice:descriptor:]` ← `Metal::init_device`); Apple `leaks` on each of its three fault children run directly: 0 leaks in 9 runs |
| S18 | `leak:MTLRegisterDevices` | Metal | Metal's process-wide device registry: the first `MTLCopyAllDevices` runs `MTLRegisterDevices` → `-[MTLIOAccelServiceGlobalContext init]`, whose global context keeps its dispatch queues and sources for the process; the device array `MTLCopyAllDevices` returns is created outside this frame, so an unreleased array stays reported | `rustty-apprt-c` `macos_window_host` (`Metal::warmup` → `choose_device` → `MTLCopyAllDevices`; 2 × 512 bytes of dispatch lanes and a 320-byte dispatch unote; timing-dependent); Apple `leaks` on the natively built binary: 0 leaks in every run |
| S19 | `leak:SLSTCCService::TCCAccessCheck` (P8-W9 FX3, N602) | SkyLight / TCC | SkyLight's one-time initialization (`__SLSInitialize_block_invoke` under `dispatch_once`, run by the first `SLSGetActiveDisplayList`, here from `CVDisplayLinkCreateWithActiveCGDisplays`) preflights the `kTCCServiceListenEvent` TCC service; TCC's XPC connection to `tccd` (`xpc_connection_create_mach_service`: its dispatch mach channel and two unotes) is reported when the exit scan does not find it held (timing-dependent). Once per process; Rustty calls no TCC API. An unreleased CVDisplayLink is not reported by LSan even without the entry (CoreVideo keeps it reachable), so S19 hides nothing there | `rustty-apprt-c` `macos_window_host`, P8-W9 T2 Mac G-LSAN run (commit `77c3ecdf6`, `.port/phase-8/w9/G-LSAN-mac.rustty-apprt-c.log`): 144 B direct + 128 B + 56 B indirect, one each, every stack `TCCAccessPreflight` ← `SLSTCCService::TCCAccessCheck` ← `__SLSInitialize_block_invoke` ← `_dispatch_once_callout` ← `SLSGetActiveDisplayList` ← `CVDisplayLinkCreateWithActiveCGDisplays` ← `DisplayLink::create_with_active_cg_displays` (render thread). Not ours and not per display link: `tccd`'s log (`log stream`) shows one ListenEvent preflight per process (msgID `<pid>.1`) for a `macos_window_host` run that creates a display link per surface; a probe that creates and releases a CVDisplayLink 1, 5 and 20 times on fresh threads leaks a constant 5,352 B (S4-S7 only) in each. Not reproduced in 23 later full `macos_window_host` runs (0 reports; GREEN with S1-S18), hence timing-dependent. Negative controls with S19 in place: a `Box::leak` scratch test in `macos_window_host` is RED (40 B reported); a probe leaking 3 CVDisplayLinks reports none with or without suppressions. Apple `leaks` was not run (it can raise a developer-tools authorization prompt) |

### G-LSAN-GTK entries (`lsan-gtk.supp`, P7-W24)

G-LSAN-GTK (`sanitizers.sh --gate lsan-gtk`; NEW-FINDINGS N502's Linux coverage gap) runs one live GTK
test that creates and closes a core `Surface` (`rustty-apprt:gtk,wayland,x11` `gtk_window_live`) under ASan
with `detect_leaks=1` inside `.port/tools/gtk_live_headless_sway.sh`, on Linux only. LSan reads ONE file,
so the lane concatenates `lsan.supp` (S1/S2 apply: the renderer's EGL) and `lsan-gtk.supp` into its own
directory. Entries here may name a third-party Rust crate as well as a system library, under the same
criteria.

**Reachability caveat and ceilings (P7-W25).** LSan treats a suppressed block as live, so an entry also hides
every block reachable only through the blocks it matches: a suppression can hide more than its own
allocation. G2 (below, removed) showed the risk: its closures captured `Rc`s of widget data, so a widget
(and the core surface it owns) leaked through a reference held by such a closure would have been reported
by nothing. Two guards stand against that now: (1) every `lsan-gtk.supp` entry has a ceiling on the
allocations it may match in the lane's run (`sanitizers.sh` `LSAN_GTK_CEILINGS`, read from LSan's
`Suppressions used` table; above the ceiling is RED, an entry without a ceiling or a ceiling without an entry
is RED; once the test has run, a log without that table, or G1, which matches in every run, matching
nothing, is RED too, so the check is never vacuous; G-SAN-SELF H arms for each); (2) the live KATs' `close_surfaces` (`tests/common/gtk_live.rs`)
asserts that the window and every Rustty widget in it (the `Ghostty*` GTypes: `Window`, `Tab`, `SplitTree`,
`SurfaceScrolledWindow`, `Surface`, `RenderSurface`, `SearchOverlay`, ...) are finalized, so a refcount leak
on any of them is RED whatever LSan's suppressions hide. G1 and G1b match fontconfig patterns, which point
only to other fontconfig data, never to Rustty objects. The lane also loads `lsan.supp`, whose Mesa entries
(S1/S2: `libEGL_mesa.so`, 215 allocations, and `libgallium`, 940, in this lane) carry the same caveat: they
are driver state of the renderer's EGL display and point into Mesa only. They have no ceiling here; guard
(2) covers what a Rustty object leaked beneath them would be.

**Negative controls.** G-SAN-SELF (Linux) runs the synthetic crate under the lane's combined file and
requires RED for a Rust `Box` leak, a `g_strdup` string, a `GObject`, an `FcPattern` from `FcNameParse` (G1)
and a sunk-and-referenced `GClosure` (N516's shape, which no entry may hide), and GREEN for a GLib
allocate-and-free control.

| id | pattern | library | reason | evidence |
|---|---|---|---|---|
| G1 | `leak:libpangoft2-1.0.so` | Pango (system), with fontconfig below it | GTK's widgets lay out text through the system Pango, whose fontconfig font map enumerates and matches fonts (`FcFontSetList`, `FcFontSetMatch`); the result patterns are unreachable at exit. Pango's calls land in the port's vendored fontconfig (the frames are `crates-sys/fontconfig-sys/vendor/src/*.c`), but no frame is Rustty's: Rustty's own font code does not go through Pango. | G-LSAN-GTK first run, rvm, 2026-10-07: 542 direct `FcPatternObjectInsertElt` reports (fcpat.c:514/506) ← `FcListAppend`/`FcFontRenderPrepare` ← `FcFontSetList`/`FcFontSetMatch` ← `libpangoft2-1.0.so.0`, plus their indirect `FcValueListCreate`/`FcStrCopy`/`FcConfigValues`/`FcRangeCreateDouble`/`FcLangSetCreate`; matched 6,913 allocations, 306,067 bytes. |
| G1b | `leak:libpangocairo-1.0.so` | Pango (system) | the same font map's patterns through its cairo layer: `libpangocairo` calls `FcConfigSubstitute` on a pattern it keeps. | second run, with G1: 10 direct `FcPatternObjectInsertElt` ← `FcConfigPatternAdd` ← `FcConfigSubstituteWithPat` ← `libpangocairo-1.0.so.0`, plus indirect; matched 2,197 allocations, 54,516 bytes. |
| ~~G2~~ | *(removed P7-W25)* `leak:^<glib::closure::Closure>::new_local::<<gtk4::builder_rust_scope::imp::BuilderRustScope as gtk4::subclass::builder_scope::BuilderScopeImpl>::create_closure::*>$` | gtk4-rs 0.11.4 (Rust crate) | gtk4-rs's `BuilderRustScope` returned already-sunk template-callback closures, and GTK's builder added a reference when it connected them and never dropped it: every widget template instantiation leaked its callback closures (NEW-FINDINGS N516). **Removed**: the port binds its template callbacks through GTK's C builder scope (`gtk_widget_class_bind_template_callback_full`, `extern "C"` trampolines; `rustty-apprt` `gtk/class/template_callback.rs`), as the reference does, and that scope's closures are floating, so nothing leaks. | was: G-LSAN-GTK first run, 93 direct `GClosure` reports, 186 allocations, 8,928 bytes. Now: G-LSAN-GTK GREEN without the entry (P7-W25; results below). |

Results (rvm, aarch64 Fedora 44, GTK 4.22.4, GLib 2.88.2, Pango system build; 2026-10-07): G-LSAN-GTK GREEN
with the N502 fix (1 test, 0 reports, 65 s for the instrumented build and the run), RED without it (one
32-byte `ArcInner` from `DrawMutex::new`). P7-W25 (N516 fixed, G2 removed): GREEN, 1 test, 0 reports;
matched G1 6,913 and G1b 2,197 allocations (ceilings 7,600 and 2,420), lsan.supp's Mesa entries 215
(`libEGL_mesa.so`) and 940 (`libgallium`). The base `4a32358ae` with only G2 removed: RED, 279 allocations,
16,144 bytes, under `BuilderRustScope` frames.

## Valgrind suppressions (`.port/tools/gates/valgrind-llvmpipe.supp`, G-VALGRIND)

G-VALGRIND (`.port/tools/gates/check_valgrind.sh`) loads the reference's `valgrind.supp` verbatim and,
after it, this port-only file. Every entry has its WHY above it in the file (D6 rule c), and the same
criteria as above apply.

| id | entry | library | reason | evidence |
|---|---|---|---|---|
| VG1 | `llvmpipe-gallivm-llvm-codegen` (`Memcheck:Cond`, `...`, `fun:gallivm_jit_function`) | Mesa llvmpipe / LLVM | conditional jumps inside LLVM's code generator while llvmpipe JIT-compiles a shader, on a stack slot of `lp_build_create_jit_compiler_for_module` | NEW-FINDINGS N412; `.port/tools/valgrind-llvmpipe-probe/probe.c` reproduces it with no Rustty code |
| VG2 | `mesa-egl-dri2-display-not-terminated` (`Memcheck:Leak`, definite only; `calloc` ← `dri2_display_create` ← `dri2_initialize` ← ... ← `eglInitialize`) | Mesa EGL | the port, like the reference (`OpenGL.zig:113-115`; DECISIONS B10), never terminates the shared surfaceless display; Mesa's at-exit teardown frees the `_EGLDisplay` but not an initialized display's driver data | NEW-FINDINGS N509. G-VALGRIND full run (rvm, P7-W18, 2026-10-07, `1e84e3ff5`): 16 `rustty-renderer` lib parts, 384 direct bytes each, under `OffscreenContext::init` (`opengl/gl/sys.rs:214`). `.port/tools/valgrind-llvmpipe-probe/egl-no-terminate.c`: `keep` 2 records, `terminate` 0; a planted 4 KiB leak stays reported with the entries loaded |
| VG3 | `mesa-egl-dri2-configs-not-terminated` (`Memcheck:Leak`, definite only; `calloc` ← `_eglCreateArray` ← `_eglLinkConfig` ← `dri2_add_config` ← ... ← `dri2_initialize` ← ... ← `eglInitialize`) | Mesa EGL | the config array of the same display, as VG2 | as VG2 (32 direct bytes, 28,000 with indirect) |

The read-back pixels of the renderer's GL tests are not suppressed: the tests that compare them declare
exactly those bytes defined (`make_mem_defined`; N412, N510).

## Sanitizer runtime options set by the lanes

Not suppressions, but they change what a lane does, so each is recorded with its reason (also next to
the option in `sanitizers.sh`).

| option | lanes | reason | evidence |
|---|---|---|---|
| `detect_leaks=0` | G-ASAN | G-LSAN (the same ASan build with `detect_leaks=1`, on macOS and Linux) is the leak lane, so G-ASAN's verdict stays memory errors. (Until P7-W16a this row said Apple's ASan has no leak checker: true of Apple clang's runtime, not of rustc's, which G-LSAN uses.) | design §B; P7-W16a |
| `detect_leaks=0` | G-HWASAN | HWASan carries LeakSanitizer on Linux; G-LSAN is the leak lane, so G-HWASAN's verdict stays memory errors (as G-ASAN's). | P7-W16b |
| `-Zexport-executable-symbols` | G-LSAN on macOS | The macOS ASan runtime is a dylib that looks the user hooks (`__lsan_is_turned_off`, `__lsan_default_suppressions`) up dynamically (`nm -m librustc-nightly_rt.asan.dylib`: "weak external `___lsan_is_turned_off` (dynamically looked up)"), and rustc exports nothing from an executable by default, so `rustty-termio`'s fork-child hook (below) was never seen. Not suppressive: it only makes the documented hook visible. On Linux the runtime is static and the hook resolves at link time. | P7-W16a: a probe whose hook always returns 1 reported its leak without the flag and not with it; G-SAN-SELF's "opt-out hook" arm is RED without the flag (mutation run) |
| `log_path=<private temp dir>/rt` (runtime output to per-process files, appended to the crate log after the run) | G-LSAN on macOS | With `lsan-darwin.supp`, `print_suppressions=1` prints its table on the stderr of every process in which a suppression matched, including CLI children whose stderr a test asserts on (`rustty`'s `cli_log_facade`: `+show-config` must print nothing). Not suppressive: every report and table still reaches the crate log the classifier reads, and a child that leaks still fails its test (exit 23). Linux keeps stderr (no Linux suppression matches in a child). | P7-W16a: run 2's `cli_log_facade` failure, stderr = the S3/S4 table only; with `log_path` the crate is GREEN (17 tests) and both tables are in its log; the probe's leak is still RED |
| `allocator_may_return_null=1` | G-ASAN, G-LSAN, G-MSAN, G-HWASAN, G-TSAN | ASan's malloc aborts on a request above its 1 TiB maximum, where the system malloc returns NULL; the port's out-of-memory tests rely on that NULL (`try_reserve` reporting failure, the port of Zig's `error.OutOfMemory` paths). It does turn off three ASan reports: allocation-size-too-big (a request over the allocator's maximum), calloc-overflow (count x size overflowing) and out-of-memory (the allocator failing): each now returns NULL to the caller instead of aborting with a report. Out-of-bounds, use-after-free, double-free, leak and uninitialized-read detection are unaffected, and a NULL the code does not handle still crashes visibly. | NEW-FINDINGS N496 item 5 |
| `RUST_MIN_STACK=8388608` | all five | Instrumented frames are 2-3x their native size; a test that checks a recursion bound fits the default 2 MiB stack overflows under ASan with no memory error. Unbounded recursion still overflows 8 MiB; the native bound is checked by `cargo test --workspace`. | N496 item 6 |
| line-tables-only debuginfo | all five | Full debuginfo made the GTK lane's instrumented test links exhaust the 10 GB VM (ld SIGBUS). Reports keep every function, file and line. | N496 item 7 |

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
| H0 | `-Zbuild-std -Ctarget-feature=+tagged-globals,-outline-atomics -Clink-arg=-fuse-ld=lld` | lane configuration | Each is needed: without `-Zbuild-std` rustc rejects the mix (`-Zsanitizer=hwaddress` is "incompatible with `-Zsanitizer` being unset in dependency `core`"); without `-outline-atomics`, compiler_builtins' outline-atomics asm takes `adrp` of a tagged global and the link fails (`relocation truncated to fit: R_AARCH64_ADR_PREL_PG_HI21 against ... HAVE_LSE_ATOMICS`, both ld.bfd and lld), so atomics are inlined instead (inline atomics, LL/SC on the baseline target); GNU ld 2.46 links tagged globals into a binary that dies with SIGILL before `main` (an invalid instruction, `.inst 0xb2ea5fe8`, in std's `args::really_init`), so the link uses lld. `+tagged-globals` is what clang turns on itself for `-fsanitize=hwaddress`; rustc warns that it is an "unknown and unstable feature". | probe crate on rvm, each failure reproduced one flag at a time |
| H1 | exclude `xtask` | crate exclusion | Its tests run nested cargo builds that inherit `RUSTFLAGS=-Zsanitizer=hwaddress` without `-Zbuild-std`, which rustc rejects. The same reason as M4. | first G-HWASAN run (rvm, 2026-10-02): 38 nested builds failed with `mixing -Zsanitizer will cause an ABI mismatch in crate rustty_util`, so 2 tests failed (`install_layout_is_versioned_like_the_reference`, `lib_version_string_versions_the_library`); N504 |
| H2 | the C-ABI smoke (`rustty-apprt-c` `c_abi_smoke_compile_link_run`) links its C program with `clang -fsanitize=hwaddress -fuse-ld=lld` under `cfg(rustty_hwasan)` | test-side configuration | The instrumented cdylib leaves `__hwasan_*` to the executable (as for ASan, N496 item 4). clang 22's HWASan runtime matches rustc's, so the test runs; under MSan it does not (the smoke is ignored there). | N504. First run: `undefined reference to __hwasan_init` (and 8 more) linking the smoke; with the flags it passes (rvm, 32 tests in the crate) |
| H3 | `gettext-sys` in `HWASAN_UNINSTRUMENTED` | archive-check exception | It compiles no C on Linux (libintl is in glibc; `crates-sys/gettext-sys/build.rs` builds the vendored gettext only on macOS), so there is no archive to check. Stale (RED) if it ever builds an instrumented archive. | `build.rs:80-81` (`target_os != "macos"` returns early) |

The C and C++ we compile through `cc` are instrumented with `clang -fsanitize=hwaddress -mno-outline-atomics`
through the same kind of wrapper as G-MSAN's (only when the crate being built is instrumented): checked on
`oniguruma-sys`, whose `libonig.a` has 438 `__hwasan_*` references and whose `onig_search` calls
`__hwasan_check_*`. The gate checks this rather than assuming it: after each `-sys` crate's build, every
`lib*.a` its build script produced must reference `__hwasan_*` (`hwasan_archive_check`), else RED, and
G-SAN-SELF's A4 C arm overflows the heap inside C compiled through `$CC` and must go RED. The one named
exception (`HWASAN_UNINSTRUMENTED`) is H3. Checked on rvm (2026-10-02, commit dcc2b2ad, one shared
target): all 16 `-sys` members, 15 with every archive instrumented (17 archives; `glslang-sys` included,
so it needs no exception) and `gettext-sys` the named skip, 59 tests GREEN; G-SAN-SELF GREEN with the C
arm RED. Uninstrumented code (system libraries, libstdc++, Mesa, GTK) needs nothing: it
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
specs (44 GREEN, 2,029 tests, 0 RED). The complete run on the fixed tree (1529049d9): G-HWASAN GREEN,
9,987 tests over all 63 specs including the three feature lanes, 0 reports, cross-check 412 ran / 56
expected absent / 0 problems, 56 min (N504).

So G-HWASAN is a complement to G-ASAN, not a replacement: G-ASAN stays the everyday lane (macOS and Linux,
deterministic), and G-HWASAN adds the wild-pointer class and instrumented C on rvm.

## ThreadSanitizer (G-TSAN, P7-W17)

G-TSAN (`sanitizers.sh --gate tsan`) runs ThreadSanitizer (`-Zsanitizer=thread -Zbuild-std`) over the
design's threaded crates (`TSAN_SCOPE`, design §B TSan row): `rustty-core` (render thread, mailbox,
surface lifecycle threads), `rustty-termio` (pty read thread, exec), `rustty-xev`, `rustty-renderer`,
`rustty-vt` with `wuffs,oniguruma` (the vt search thread, the tmux threads, the oniguruma first-use init
race test; the default-feature tests run too), `rustty-vt-c` and `highway-sys` (the SIMD dispatch state
every thread shares; UD377's regression test). Its scope is an allow-list because a race needs two
threads: the rest of the workspace runs its threads (if any) through these crates or not at all.
macOS and Linux; BLOCKED on any other host. It has run on macOS (P7-W17) and on the aarch64 Fedora VM
`rvm` (P7-W24, V32's Linux half).

| id | what | kind | reason | evidence |
|---|---|---|---|---|
| T0 | `-Zbuild-std`; the cc-built C/C++ compiled with `clang -fsanitize=thread` through the `tsan-cc`/`tsan-c++` wrappers (only when the crate being built is instrumented) | lane configuration | Without `-Zbuild-std` rustc refuses the mix ("mixing `-Zsanitizer` will cause an ABI mismatch"). Uninstrumented C would hide both its own races and its synchronization (a lock taken in C is invisible to TSan, so Rust accesses ordered only by it would be reported). The gate checks the C is instrumented rather than assuming it: after each crate every `lib*.a` built for the instrumented target must reference `__tsan_*` (`tsan_archive_check`; exceptions `TSAN_UNINSTRUMENTED`, none so far), else RED. | P7-W17 probe crate (macOS); the gate run: 20 archives, all instrumented. G-SAN-SELF's A5 C arm (a race between two threads inside C compiled through `$CC`) is RED, which it could not be with uninstrumented C. |
| T1 | `allocator_may_return_null=1` | runtime option | TSan's `malloc` aborts on a request above its 1 TiB maximum like ASan's; the port's out-of-memory tests rely on the NULL (as for the other lanes, N496 item 5). | first G-TSAN pass, `rustty-vt`: `ThreadSanitizer: requested allocation size 0x7fffffffffffffff exceeds maximum supported size` (`allocation-size-too-big`), stopping the binary after 41 tests. |
| T2 | `second_deadlock_stack=1` | runtime option | Not suppressive: a lock-order-inversion report shows both acquisitions. | — |
| T3 | `.port/phase-2/tsan.supp` through `suppressions=…:print_suppressions=1` | suppressions file | **Empty.** Every entry needs the criteria at the top of this file plus a negative control (a race of ours through the same API must stay RED). The first runs needed none: the only report on macOS was UD377, a real race in vendored Highway, fixed rather than suppressed; the first Linux run's reports are T4's. | — |
| T4 | `rustty-xev`'s tests run one at a time (`TSAN_SERIAL`; `--test-threads=1` for that spec only) | lane configuration (Linux reports; P7-W24, NEW-FINDINGS N514) | TSan models every fd as a word written by the call that opens it (`socket`, `open`, ...) and by `close`, and checks `epoll_ctl`, `mmap`, `close` and the rest against it. It sees neither the io_uring arm's close ops (the kernel closes the socket) nor fds opened by raw syscalls (`SYS_pidfd_open` in the epoll arm's `Process`, `io_uring_setup`). With libtest running tests side by side, one test's socket number, closed unseen, is reopened unseen by another, and the second test's `epoll_ctl`/`mmap`/`close` on it is reported against the first test's `socket()`. No memory is shared. A suppression was tried and rejected: TSan matches `race_top` against the raw interceptor name (`___interceptor_mmap`, not the printed `mmap`), and covering `close` would also hide a real double close. Running that crate's tests one at a time removes only libtest's side-by-side running of DIFFERENT tests, which the reference's serial Zig test runner never does; the threads a test starts (thread pool, async wakeups, the pty and process watchers) are still checked against each other. Linux only: macOS's kqueue arm has no unseen close or raw-syscall fd, so `spec_args` applies `TSAN_SERIAL` only when `$OS` is Linux (P7-W25; G-SAN-SELF H arms for Linux and macOS). A `TSAN_SERIAL` spec outside `TSAN_SCOPE` is RED. | rvm, P7-W24, the TSan `rustty-xev` lib test binary with an EMPTY suppressions file: run in parallel, 59 reports in 24 of 25 runs and 26 in 15 of 25 in a second set (shapes: `epoll_ctl` ← `epoll::Loop::epoll_register` and `mmap` ← `io_uring::ring::Sq::init`, each against `socket` ← `io_uring::{udp::UDP,tcp::TCP}::init` of another test; `close` against `socket`); one at a time, 0 reports in 25 runs. G-SAN-SELF "A5s": a race between one test's own threads is RED with `--test-threads=1`. |

No Metal, CoreText, libdispatch or other system-framework report appeared on macOS, so nothing is
suppressed there.

**Runs (macOS, aarch64-apple-darwin, nightly 2026-07-22 / LLVM 22.1.8, Apple clang 21, one shared
target dir).** The first pass (P7-W17) found UD377 in `rustty-vt-c` (a data race between two formatter
tests' first SIMD dispatch, `hwy::ChosenTarget::LoadMask` against `StoreMask`, vendored Highway) and T1
in `rustty-vt`; `rustty-core` (82), `rustty-termio` (126), `rustty-xev` (54) and `rustty-renderer` (143)
were GREEN. With both fixed, the gate: **G-TSAN GREEN, 4,380 tests, 0 reports, 38 min** (`rustty-vt`
3,370 in 1,781 s, `rustty-vt-c` 586, `highway-sys` 19, `rustty-termio` 126 in 339 s; 20 C archives
instrumented). The per-crate hang bound for TSan is 3,600 s, 2x the slowest crate. NEW-FINDINGS N508.

**Runs (Linux, rvm: aarch64 Fedora 44, glibc 2.43, nightly 2026-07-17, clang 22; P7-W24, V32).** The first
pass was RED on `rustty-xev` only (T4's fd-model reports, N514); every other spec GREEN. With `TSAN_SERIAL`:
**G-TSAN GREEN, 4,428 tests, 0 reports, 20.75 min** (`rustty-vt` 3,366 in 1,059 s, `rustty-vt-c` 586,
`rustty-renderer` 154, `rustty-xev` 111 one at a time in 11 s, `rustty-termio` 109, `rustty-core` 83,
`highway-sys` 19; 17 C archives instrumented, Linux's set: `gettext-sys` builds no libintl there). No
system-library report on Linux either, so `tsan.supp` stays empty.

## Crate exclusions and empty lanes

No workspace member is excluded from G-ASAN or G-LSAN; G-HWASAN excludes `xtask` (H1). The G-LSAN matrix behind fix round 1's numbers ran on commit 7ddf62ffd (10,111 tests; its cross-check RED only for the then-missing Linux expected-absent rows); the full G-LSAN gate re-ran GREEN on 05478107 (10,111 tests, 18 min, cross-check 412 ran / 56 expected absent / 0 problems). `NO_TESTS_ON` in `sanitizers.sh` lists the members
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
| ThreadSanitizer | **runs** as G-TSAN (P7-W17; see "ThreadSanitizer (G-TSAN)" below) | `-Zsanitizer=thread` with `-Zbuild-std` (rustc's target spec and the unstable book list `aarch64-apple-darwin`; compiler-rt `COMPILER_RT_HAS_TSAN` includes Darwin). Probe (P7-W17, nightly 2026-07-22): a deliberate race is reported (`WARNING: ThreadSanitizer: data race`, exit 66), a Mutex-ordered control passes; without `-Zbuild-std` rustc refuses ("mixing `-Zsanitizer` will cause an ABI mismatch"). |
| HWAddressSanitizer | **unsupported** | compiler-rt builds it for `Linux|Android|Fuchsia` only (`COMPILER_RT_HAS_HWASAN`); rustc: "hwaddress sanitizer is not supported for this target"; Apple clang: "unsupported option '-fsanitize=hwaddress'". Handled on Linux by a separate task. |

Leak-checking alternatives evaluated (P7-W16a):

- **Apple `leaks --atExit -- <test binary>`** works on native test binaries: RED on the probe's leak
  (64 ROOT LEAKs, exit 1), GREEN on the clean test (exit 0); quiet on `rustty-font` (89 tests) and the
  `rustty-apprt` unit tests (0 leaks); about 1.3x native wall time (`rustty-font`: 6.6 s native, 8.5 s
  under `leaks`, 10.0 s with `MallocStackLogging=1`). It scans only the main process at exit (not forked
  children) and needs `MallocStackLogging` for allocation stacks. Used here as the ARBITER for the S3-S18
  suppressions, not as a separate gate: G-LSAN already gives the same detection, stacks, the
  suppression file, the Miri-exception cross-check and the existing self-tests.
- **Homebrew LLVM compiler-rt**: not usable on macOS 26 (above).
- **A tracking allocator**: PORT_RULES R6 removed that approach in favour of Miri plus the sanitizers;
  with a working LSan on macOS it is not needed.

## Closed limit: the ASan deadlock around `fork` on macOS (N506)

Under load, `rustty-apprt-c`'s `macos_window_host` could hang forever under G-ASAN / G-LSAN: the io
thread's `Command::startPosix` → `fork` → `libSystem_atfork_prepare` → `_malloc_fork_prepare` spun in
`__sanitizer::StaticSpinMutex::LockSlow` while another thread spun on the same kind of lock in
`wrap_free` → `__asan::Allocator::QuarantineChunk` (NEW-FINDINGS N506; LLVM D33325's allocator locking
around `fork`). Closed in P7-W21: in ASan builds on macOS (`cfg(rustty_asan)`, set by
`rustty-termio`'s build.rs from `CARGO_CFG_SANITIZE`), `Command::startPosix` spawns the pty child with
`posix_spawn`, which runs no atfork handler. Normal builds keep the reference's `fork`. The ASan child
has no controlling terminal (macOS has no `posix_spawn` form of `TIOCSCTTY`); see
`Command::asan_posix_spawn`. The per-crate hang bound (`SAN_CRATE_TIMEOUT_S`) stays as a general safety
net.

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
