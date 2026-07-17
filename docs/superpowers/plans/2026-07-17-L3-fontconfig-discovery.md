# L3 — Linux font discovery (Fontconfig) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a real system font resolve on Linux via Fontconfig, replacing the three
`unimplemented!()` `DeferredFace` leaves (`deferred_face.rs:479,482,489`) and the empty
`discovery::Fontconfig` stub with a faithful port of ghostty's Fontconfig discoverer —
proved by an end-to-end KAT that discovers `monospace`, loads it through the existing
FreeType face, and resolves a glyph. This also fixes the pre-existing HARD Linux compile
errors in `rustty-font-shaper` (an `Options` type split between `face::Options` and
`face::freetype::Options`, a missing `discovery::Error` on non-macOS, and a
method-less `Fontconfig` stub that `shared_grid_set.rs` already calls unconditionally),
which today block `cargo build -p rustty-font-shaper` — and therefore
`cargo build --workspace` — on Linux.

**Architecture:** Mirror ghostty's split exactly: (1) `pkg/fontconfig/*.zig` is ghostty's
own hand-written Zig binding package over the C `fontconfig` library — ported here as a
new, self-contained `crate::fc` module (sibling of the existing `crate::ft`/`crate::hb`
thin-FFI-wrapper modules) over a new vendored `fontconfig-sys` crate. (2)
`src/font/discovery.zig`'s `Fontconfig`/`Descriptor.toFcPattern` is the discovery layer —
ported into the existing `discovery.rs`, replacing its DEFERRED comment blocks, and built
on top of `crate::fc`. (3) `src/font/DeferredFace.zig`'s `fc` payload + the three
backend arms (`hasCodepoint`/`familyName`+`name`/`load`→`loadFontconfig`) — ported into
`deferred_face.rs`, replacing the `unimplemented!()` leaves and the `_deferred: ()`
placeholder field. Fontconfig itself needs libxml2 to parse `/etc/fonts/fonts.conf` (the
reference vendors both under `ENABLE_LIBXML2`), so a new vendored `libxml2-sys` lands
first as `fontconfig-sys`'s build dependency, alongside the already-vendored
`freetype-sys` (fontconfig's `FcFreeTypeCharSetAndSpacing` arm parses font files via
FreeType). Both new `-sys` crates follow the PROVEN `crates-sys/{freetype-sys,
harfbuzz-sys}` shape (vendor the same upstream C ghostty vendors + `cc` + `bindgen` +
`links=` + a KAT) — no pkg-config against the system fontconfig/libxml2, even though both
are present on this host (2.17 / 2.12): R3 requires the SAME upstream C source ghostty
itself builds, standalone.

**Tech Stack:** Rust, the vendored `fontconfig` C library (ghostty pins **2.14.2**,
`deps.files.ghostty.org/fontconfig-2.14.2.tar.gz`, per `pkg/fontconfig/build.zig.zon`)
built via `cc` + FFI via `bindgen`, the vendored `libxml2` C library (ghostty pins
**2.11.5**, `deps.files.ghostty.org/libxml2-2.11.5.tar.gz`, per
`pkg/libxml2/build.zig.zon`) built the same way, linked against the already-vendored
`freetype-sys`. Reference oracle = ghostty pinned `4749c4e9`
(`src/font/discovery.zig`, `src/font/DeferredFace.zig`, `pkg/fontconfig/**`,
`pkg/libxml2/**`).

## Global Constraints

- **R1 fidelity:** every quoted `test{}` in `discovery.zig`'s Fontconfig arm and
  `DeferredFace.zig`'s `fc` arm, PLUS every quoted `test{}` in `pkg/fontconfig/*.zig`
  (11 tests: `version`/`init`/`initLoadConfig`/`initLoadConfigAndFonts` in `init.zig`;
  `create`/`name parse` in `pattern.zig`; `cval` in `common.zig`; `create` in
  `char_set.zig`; `create` in `font_set.zig`; `create`/`hasLang exact match` in
  `lang_set.zig`) gets a Rust `#[test]`, ported verbatim in behavior. Cite the exact
  reference `file:line` per task below.
- **R3 self-contained, no pkg-config against system libs:** `fontconfig-sys` and
  `libxml2-sys` vendor the SAME upstream C sources ghostty vendors (URLs above), compiled
  standalone via `cc`, never linking the host's system `fontconfig`/`libxml2` (present at
  2.17/2.12 on this box but NOT used — parity target is ghostty's pinned 2.14.2/2.11.5).
  `fontconfig-sys → {freetype-sys, libxml2-sys}` (link-only deps, mirroring how
  `harfbuzz-sys → freetype-sys`); `libxml2-sys → {}` (leaf). Keep
  `.port/tools/gates/check_acyclic.sh` green.
- **R4 identity:** crate names are `fontconfig-sys`/`libxml2-sys` (the port identity);
  `links = "fontconfig"` / `links = "xml2"` (the archive names, matching the reference's
  `b.addLibrary(.{.name = "fontconfig"/"xml2", ...})`).
- **Compile-fix-first invariant:** Task 1 must land BEFORE any other task in this plan —
  every later task assumes `cargo build -p rustty-font-shaper` is green on Linux with
  honest `unimplemented!()` leaves (not hard compile errors). This is also a prerequisite
  the L1 plan's Task 9 (`cargo build --workspace` on Linux) implicitly needs, since
  `rustty-font-shaper` is a workspace member today.
- **No new placeholder types:** the `face::Options`/`face::FreetypeLoadFlags` fix (Task 1)
  must ALIAS to `face::freetype::{Options, FreetypeLoadFlags}` on freetype-active targets,
  not redeclare a third lookalike type — mirrors the reference, where `font.face.Options`
  IS (comptime) the freetype-active type; there is only ever one `Options` type per Zig
  build.
- **Reference pin:** all `discovery.zig`/`DeferredFace.zig`/`pkg/fontconfig/*.zig`/
  `pkg/libxml2/*.zig` citations resolve against `/home/gael/projects/ghostty` @ `4749c4e9`.
  All `rustty-font-shaper …:NNN` / `crates-sys/…` citations resolve against
  `/home/gael/projects/rustty-gtk` at plan-authoring time.

## File structure

| File | Responsibility |
|---|---|
| `crates/rustty-font-shaper/src/face/mod.rs` (**modify**) | Fix the `Options`/`FreetypeLoadFlags` 3-way cfg split (macOS / wasm32 / freetype-active) so the freetype-active arm ALIASES `face::freetype::{Options, FreetypeLoadFlags}` instead of redeclaring a placeholder. |
| `crates/rustty-font-shaper/src/face/freetype.rs` (**modify**) | Add a `const DEFAULT: FreetypeLoadFlags` (freetype's existing `Default` impl delegates to it) so `mod.rs` can name it in a `const` context. |
| `crates/rustty-font-shaper/src/discovery.rs` (**modify**) | Replace the DEFERRED `toFcPattern`/`Fontconfig`/`DiscoverIterator` comment blocks with real ports; add the Linux `discovery::Error` type; flesh out the 2 skip-stub `#[test]`s. |
| `crates/rustty-font-shaper/src/deferred_face.rs` (**modify**) | Replace the non-macOS `_deferred: ()` placeholder with a real `fc: Option<FontconfigData>` payload; wire the 3 `unimplemented!()` arms + `familyName`/`name` fontconfig arms; flesh out the `fontconfig` KAT. |
| `crates/rustty-font-shaper/src/fc.rs` (**create**) | Thin, faithful FFI wrapper over `fontconfig-sys` — port of `pkg/fontconfig/{main,init,config,pattern,common,char_set,font_set,lang_set,value,error}.zig` (11 tests). Sibling of the existing `ft.rs`/`hb.rs`. |
| `crates/rustty-font-shaper/src/lib.rs` (**modify**) | `pub mod fc;` gated `#[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]`. |
| `crates/rustty-font-shaper/Cargo.toml` (**modify**) | New `[target.'cfg(not(any(target_os = "macos", target_arch = "wasm32")))'.dependencies]` table: `fontconfig-sys = { path = "../../crates-sys/fontconfig-sys" }`. |
| `crates-sys/libxml2-sys/{Cargo.toml,build.rs,src/lib.rs,vendor/**}` (**create**) | Vendored libxml2 2.11.5, port of `pkg/libxml2/build.zig` `buildLib` (cc + bindgen + KAT). |
| `crates-sys/fontconfig-sys/{Cargo.toml,build.rs,src/lib.rs,vendor/**}` (**modify from stub**) | Vendored fontconfig 2.14.2, port of `pkg/fontconfig/build.zig` `buildLib` (cc + bindgen + KAT), linking `freetype-sys` + `libxml2-sys`. |
| `Cargo.toml` (workspace root) (**modify**) | Add `"crates-sys/libxml2-sys"` to `members`. |
| `.port/tools/gates/check_test_counts.py` (**modify**) | Add a `font_shaper_fc` manifest row (`pkg/fontconfig/*.zig` → `fc.rs`, 11 tests). |
| `.port/phase-2/DEFERRALS.md` (**modify**) | Mark the `deferred_face.rs:479/482/489` entry (added 2026-07-10) RESOLVED, cross-reference this plan's commits. |

---

### Task 1: Make `rustty-font-shaper` compile on Linux (fix the 3 hard errors)

**Scope:** NO new Fontconfig behavior yet — only make the crate type-check on Linux with
HONEST `unimplemented!()` leaves, exactly like `deferred_face.rs:472-491` already does.
This unblocks every later task (which all assume a green `cargo build`) and, per the
Global Constraints, is a prerequisite for the L1 plan's workspace-wide Linux build.

**Files:**
- Modify: `crates/rustty-font-shaper/src/face/mod.rs:120-150` (the `FreetypeLoadFlags`/
  `Options` cfg split)
- Modify: `crates/rustty-font-shaper/src/face/freetype.rs:110-142` (add a `const DEFAULT`)
- Modify: `crates/rustty-font-shaper/src/discovery.rs:60-69` (the `Discover`/`Fontconfig`
  stub) — add a properly-typed, still-`unimplemented!()` `Fontconfig` + `DiscoverIterator`
  + a real `discovery::Error` enum for the `not(macos) & not(wasm32)` target class

**Interfaces:**
- Consumes: `crate::face::freetype::{Options, FreetypeLoadFlags}` (already fully
  implemented, `face/freetype.rs:102-142`); `crate::library::Library` (already
  `FreetypeLibrary` on Linux, `library.rs:32-34`).
- Produces:
  - `face::Options`/`face::FreetypeLoadFlags` now resolve, on every target that is
    neither `macos` nor `wasm32`, to the SAME type as `face::freetype::{Options,
    FreetypeLoadFlags}` (a `pub use`, not a redeclaration) — so `DeferredFace::load`'s
    `opts: crate::face::Options` parameter is directly passable to
    `crate::face::Face::init_file`/`::init` (`face::Face` already `= freetype::Face` on
    this target class, `face/mod.rs:81-82`).
  - `discovery::Error` (Linux/other): an enum wide enough for `shared_grid_set.rs:94`'s
    `Discovery(discovery::Error)` variant + its `impl From<discovery::Error> for Error`
    (`shared_grid_set.rs:108-112`) to compose; start with the shape mirroring the macOS
    `coretext_impl::Error` (`discovery.rs:472-479`: a `Face(FaceError)` variant + a
    catch-all), fully fleshed out in Task 6.
  - `discovery::Fontconfig` gains real method SIGNATURES (`init(lib: Library) ->
    Fontconfig`, `deinit(&mut self)`, `discover(&self, desc: Descriptor) ->
    Result<DiscoverIterator, Error>`, `discover_fallback(&self, collection: *mut
    Collection, desc: Descriptor) -> Result<DiscoverIterator, Error>`) with
    `unimplemented!("Fontconfig discovery is DEFERRED to L3 Task 6/7")` bodies, matching
    the exact honesty convention already used at `deferred_face.rs:472-491`. Add a
    matching `discovery::DiscoverIterator` stub (`next`/`deinit`).

- [ ] **Step 1: Write the failing test** — a new `#[test] fn linux_font_shaper_compiles()`
  is not meaningful (this is a compile-level fix); instead widen the EXISTING skip-stub
  tests' preconditions: temporarily change
  `crates/rustty-font-shaper/src/shared_grid_set.rs`'s test module (or add a new
  `#[cfg(not(any(target_os = "macos", target_arch = "wasm32")))] #[test] fn
  discover_impl_typechecks()` compile-only smoke test that calls
  `discovery::Fontconfig::init(lib)` then `.deinit()`) to PROVE the current state fails to
  compile: `cargo build -p rustty-font-shaper` (native Linux host, no `--target` needed).
- [ ] **Step 2: Run to verify it fails** — `cargo build -p rustty-font-shaper` → FAIL
  with exactly the three errors: (a) `discovery::Error` not found (used by
  `shared_grid_set.rs:94,108`), (b) no method `init`/`deinit`/`discover`/
  `discover_fallback` on `discovery::Fontconfig` (used by `shared_grid_set.rs:623`), (c)
  (once (a)/(b) are naively patched) a type mismatch passing `crate::face::Options` where
  `crate::face::freetype::Options` is expected, the moment any call site tries to bridge
  `DeferredFace::load`'s `opts` into `Face::init_file`.
- [ ] **Step 3: Implement**:
  1. In `face/mod.rs`, replace the 2-way `#[cfg(target_os = "macos")]` /
     `#[cfg(not(target_os = "macos"))]` split on `FreetypeLoadFlags`/`FREETYPE_LOAD_FLAGS_
     DEFAULT` (lines 120-136) and the unconditional `Options` struct (lines 143-150) with
     a 3-way split: macOS keeps its own `Options`/`FreetypeLoadFlags = ()` (unchanged);
     `#[cfg(target_arch = "wasm32")]` keeps its OWN `Options`/placeholder
     `FreetypeLoadFlags` struct (unchanged behavior, needed because `face/web_canvas.rs`
     names `super::Options` directly and the `freetype` module is not compiled on wasm32,
     `face/mod.rs:24-25`); everyone else (`not(any(macos, wasm32))`) does
     `pub use freetype::{FreetypeLoadFlags, Options};` + `pub const
     FREETYPE_LOAD_FLAGS_DEFAULT: FreetypeLoadFlags = freetype::FreetypeLoadFlags::DEFAULT;`.
  2. In `face/freetype.rs`, add `impl FreetypeLoadFlags { pub const DEFAULT: Self = Self {
     hinting: true, force_autohint: false, monochrome: false, autohint: true, light: true
     }; }` right above the existing `impl Default for FreetypeLoadFlags` (line 120), then
     change that `impl Default` body to `fn default() -> Self { Self::DEFAULT }`
     (behavior-preserving refactor — same literal values, now also reachable as a
     `const`).
  3. In `discovery.rs`, replace the empty `pub struct Fontconfig;` (line 69) with the
     typed-stub struct + `unimplemented!()` methods and the `discovery::Error` enum
     described in Interfaces above. Import `crate::face::{FaceError}` /
     `crate::library::Library` / `crate::collection::Collection` as needed (mirror the
     wasm_noop module's imports, `discovery.rs:90-95`, as the template for what a Linux
     `Fontconfig`/`DiscoverIterator`/`Error` skeleton needs).
- [ ] **Step 4: Run to verify it passes** — `cargo build -p rustty-font-shaper` → GREEN;
  `cargo test -p rustty-font-shaper` → GREEN (all existing tests, including the
  macOS-only `#[cfg(target_os = "macos")]` ones, remain compiled-out and unaffected on
  Linux; re-run on a macOS host, or at minimum diff-review, to confirm the macOS
  `Options`/`FreetypeLoadFlags` arm is byte-unchanged). Remove the throwaway smoke test
  from Step 1 (or fold it into `discovery.rs`'s own test module as a permanent
  `#[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]` compile-touch test).
- [ ] **Step 5: Commit** — `git commit -m "rustty-font-shaper: fix Linux hard compile
  errors (Options alias, discovery::Error, typed Fontconfig stub) (L3 Task 1)"`

---

### Task 2: Vendor `libxml2-sys` (libxml2 2.11.5 — cc + bindgen + KAT)

**Files:**
- Create: `crates-sys/libxml2-sys/{Cargo.toml,build.rs,wrapper.h,src/lib.rs}`
- Create: `crates-sys/libxml2-sys/vendor/**` (vendored libxml2 2.11.5 sources + the two
  ghostty override files)
- Modify: `Cargo.toml` (workspace root) — add `"crates-sys/libxml2-sys"` to `members`
  (adjacent to `"crates-sys/fontconfig-sys"`, `Cargo.toml:113`)

**Interfaces:**
- Consumes: nothing (leaf `-sys` crate, like `zlib-sys`).
- Produces: `libxml2_sys::*` — the bindgen FFI surface over `<libxml/parser.h>` +
  `<libxml/tree.h>` (whatever the wrapper.h includes; start minimal — `xmlReadMemory`,
  `xmlDocGetRootElement`, `xmlFreeDoc`, `xmlNodeGetContent`, `xmlCleanupParser`, the node/
  doc structs). `links = "xml2"`; `cargo:include=<vendor/include>` for downstream
  `fontconfig-sys` (`DEP_XML2_INCLUDE`).

- [ ] **Step 1: Write the failing test** — `#[test] fn kat_libxml2_2_11_5_parse_memory()`
  in `src/lib.rs`: build a small embedded XML string (e.g.
  `b"<root><a>1</a></root>\0"`), call `xmlReadMemory` + walk to the root element name +
  `xmlFreeDoc` + `xmlCleanupParser`; assert the root element name is `"root"`.
- [ ] **Step 2: Run to verify it fails** — `cargo test -p libxml2-sys` → FAIL (crate
  doesn't exist yet / build.rs empty).
- [ ] **Step 3: Implement**:
  1. Fetch libxml2 **2.11.5** sources — per `pkg/libxml2/build.zig.zon:8`
     (`https://deps.files.ghostty.org/libxml2-2.11.5.tar.gz`); if unreachable, fall back
     to the upstream `GNOME/libxml2` `v2.11.5` release tarball (R3: do NOT substitute a
     different version). Vendor into `crates-sys/libxml2-sys/vendor/` + `LICENSE`.
  2. Copy ghostty's two override files verbatim into the vendor tree (they are the
     pre-generated config the upstream `./configure` would otherwise produce; NOT
     ghostty-authored library code, so vendoring them is R3-faithful build-config, same
     class as freetype-sys's own `ftoption`/`ftconfig` handling):
     `pkg/libxml2/override/config/posix/config.h` →
     `crates-sys/libxml2-sys/vendor/override/config/posix/config.h`;
     `pkg/libxml2/override/include/libxml/xmlversion.h` →
     `crates-sys/libxml2-sys/vendor/override/include/libxml/xmlversion.h`.
  3. `build.rs`: port `pkg/libxml2/build.zig`'s `buildLib` (lines 9-119) — a `cc::Build`
     with `.include(vendor/include)`, `.include(vendor/override/include)`,
     `.include(vendor/override/config/posix)` (off-Windows), the verbatim flag list
     (`build.zig:30-65`: `-DLIBXML_VERSION=21105 -DLIBXML_VERSION_STRING=\"21105\"
     -DLIBXML_VERSION_EXTRA=\"\" -DLIBXML_DOTTED_VERSION=\"2.11.5\" -DSEND_ARG2_CAST=
     -DGETHOSTBYNAME_ARG_CAST= -DGETHOSTBYNAME_ARG_CAST_CONST= -DLIBXML_STATIC=1
     -DLIBXML_AUTOMATA_ENABLED=1 -DWITHOUT_TRIO=1`, plus off-Windows
     `-DHAVE_ARPA_INET_H=1 ... -DHAVE_SYS_TYPES_H=1`, `build.zig:49-65`), the default-ON
     `Options` set's `-DLIBXML_{FIELD}_ENABLED=1` defines (`build.zig:68-98` — reproduce
     the DEFAULT values only: `c14n`/`catalog`/`debug`/`history`/`html`/`iconv`/
     `iso8859x`/`minimum`/`output`/`pattern`/`push`/`reader`/`regexp` (+ its
     `-DLIBXML_UNICODE_ENABLED=1` companion, `build.zig:88-90`)/`sax1`/`schemas`/
     `schematron`/`thread` (+ `-DHAVE_LIBPTHREAD=1`, `build.zig:94-96`)/`tree`/`valid`/
     `writer`/`xinclude`/`xpath`/`xptr`/`modules`; `ftp`/`icu`/`legacy`/`mem_debug`/
     `run_debug`/`thread_alloc`/`lzma`/`zlib`/`xptr_locs` default OFF — skip them), the
     `srcs` list verbatim (`build.zig:182-227`, 42 files), `.compile("xml2")`.
  4. `bindgen::Builder` over `wrapper.h` (`#include <libxml/parser.h>` +
     `#include <libxml/tree.h>`), `clang_arg` the same 3 include dirs, allowlist
     `xml.*|xmlDoc.*|xmlNode.*|_xml.*` (tune to whatever the KAT + fontconfig's
     `fcxml.c` actually need — keep it minimal, widen only if fontconfig-sys's build in
     Task 3 needs more). `cargo:include=<vendor/include>`.
- [ ] **Step 4: Run to verify it passes** — `cargo build -p libxml2-sys` → GREEN;
  `cargo test -p libxml2-sys` → GREEN (KAT parses the embedded XML, root name asserts
  `"root"`).
- [ ] **Step 5: Commit** — `git commit -m "rustty: vendor libxml2-sys (libxml2 2.11.5,
  cc+bindgen+KAT) — R3 (L3 Task 2)"`

---

### Task 3: Vendor `fontconfig-sys` (fontconfig 2.14.2 — cc + bindgen + KAT)

**Files:**
- Modify (from stub): `crates-sys/fontconfig-sys/{Cargo.toml,build.rs,src/lib.rs}`
- Create: `crates-sys/fontconfig-sys/wrapper.h`
- Create: `crates-sys/fontconfig-sys/vendor/**` (vendored fontconfig 2.14.2 sources + the
  ghostty override files)

**Interfaces:**
- Consumes: `freetype-sys` (`DEP_FREETYPE_INCLUDE`, link-only dep — mirrors
  `harfbuzz-sys`'s exact wiring, `crates-sys/harfbuzz-sys/Cargo.toml:49` +
  `build.rs:43-47`), `libxml2-sys` (`DEP_XML2_INCLUDE`, link-only dep, same pattern —
  Task 2).
- Produces: `fontconfig_sys::*` — the bindgen FFI surface over
  `<fontconfig/fontconfig.h>` (`FcInit`, `FcInitLoadConfigAndFonts`, `FcPatternCreate`/
  `Add`/`Get`/`Destroy`, `FcCharSetCreate`/`AddChar`/`HasChar`/`Destroy`,
  `FcLangSetCreate`/`AddLang`/`HasLang`/`Destroy`, `FcFontSetCreate`/`Destroy`,
  `FcConfigSubstitute`, `FcDefaultSubstitute`, `FcFontSort`, `FcFontRenderPrepare`, the
  `FcType`/`FcResult`/`FcMatchKind`/`FC_*` property-name macros, `FC_WEIGHT_*`/
  `FC_SLANT_*`/`FC_PROPORTIONAL`/`FC_MONO`/etc). `links = "fontconfig"` (already set in
  the stub `Cargo.toml`).

- [ ] **Step 1: Write the failing test** — `#[test] fn kat_fontconfig_2_14_2_init_and_pattern()`
  in `src/lib.rs`, mirroring `pkg/fontconfig/init.zig`'s `"init"`/`"initLoadConfigAndFonts"`
  tests (lines 30-43) + `pattern.zig`'s `"create"` test (lines 147-161): `FcInit()` →
  true; `FcInitLoadConfigAndFonts()` → non-null `*FcConfig`; `FcPatternCreate()` →
  non-null; `FcPatternAdd(pat, FC_FAMILY, monospace-string-value, FcFalse)` → `FcTrue`;
  `FcPatternGet(pat, FC_FAMILY, 0, &val)` → `FcResultMatch` with `val.u.s == "monospace"`;
  `FcPatternDestroy`/`FcConfigDestroy`.
- [ ] **Step 2: Run to verify it fails** — `cargo test -p fontconfig-sys` → FAIL (the
  current stub `build.rs` compiles nothing, `src/lib.rs` has no FFI).
- [ ] **Step 3: Implement**:
  1. Fetch fontconfig **2.14.2** sources — per `pkg/fontconfig/build.zig.zon:8`
     (`https://deps.files.ghostty.org/fontconfig-2.14.2.tar.gz`); upstream
     `freedesktop/fontconfig` `2.14.2` release tarball as the R3-faithful fallback.
     Vendor into `crates-sys/fontconfig-sys/vendor/` + `COPYING`.
  2. Copy ghostty's 5 override headers verbatim into
     `crates-sys/fontconfig-sys/vendor/override/include/`:
     `fcalias.h`/`fcaliastail.h`/`fcftalias.h`/`fcftaliastail.h` (all empty — they exist
     only so `#include "fcalias.h"` resolves when fontconfig's internal alias-export
     machinery is disabled for a static, non-exported build) + `fcobjshash.h` (347 lines
     — the pre-generated `gperf` object-name hash table, avoiding a `gperf` build-time
     dependency).
  3. `build.rs`: port `pkg/fontconfig/build.zig`'s `buildLib` (lines 61-261) — a
     `cc::Build` with `.include(vendor/override/include)`, the verbatim flag list
     (`build.zig:87-129`: the `-DHAVE_*` set, `-DFLEXIBLE_ARRAY_MEMBER`,
     `-DHAVE_STDATOMIC_PRIMITIVES`, `-DFC_GPERF_SIZE_T=size_t`,
     `-Wno-implicit-function-declaration -Wno-int-conversion
     -fno-sanitize=undefined -fno-sanitize-trap=undefined`), the pointer-width defines
     (`build.zig:131-143`: `-DSIZEOF_VOID_P=8 -DALIGNOF_VOID_P=8` on 64-bit), the
     non-Windows block (`build.zig:151-194`: `-DHAVE_FSTATFS ... -DHAVE_SYS_STATFS_H
     -DHAVE_SYS_VFS_H` on Linux specifically, plus
     `-DFC_CACHEDIR=\"/var/cache/fontconfig\"
     -DFC_DEFAULT_FONTS=\"<dir>/usr/share/fonts</dir><dir>/usr/local/share/fonts</dir>\"
     -DFC_TEMPLATEDIR=\"/usr/share/fontconfig/conf.avail\"
     -DFONTCONFIG_PATH=\"/etc/fonts\" -DCONFIGDIR=\"/usr/local/fontconfig/conf.d\"` — the
     non-FreeBSD arm), the libxml2 enablement block (`build.zig:216-227`:
     `-DENABLE_LIBXML2 -DLIBXML_STATIC -DLIBXML_PUSH_ENABLED`), the `srcs` list verbatim
     (`build.zig:269-298`, 27 files), `.include(DEP_FREETYPE_INCLUDE)`,
     `.include(DEP_XML2_INCLUDE)`, `.compile("fontconfig")`.
  4. Add the `freetype-sys`/`libxml2-sys` `[dependencies]` entries to
     `crates-sys/fontconfig-sys/Cargo.toml` (link-only, mirror the doc-comment style of
     `crates-sys/harfbuzz-sys/Cargo.toml:42-49`).
  5. `bindgen::Builder` over `wrapper.h` (`#include <fontconfig/fontconfig.h>`, per
     `pkg/fontconfig/c.zig:1-3` — the ENTIRE binding surface is this one header),
     `clang_arg` the override-include + `DEP_FREETYPE_INCLUDE` + `DEP_XML2_INCLUDE` dirs,
     allowlist `Fc.*|FC_.*`. `cargo:include=<vendor/override/include + upstream fontconfig
     dir>` for completeness (no downstream consumer needs it beyond this plan's `fc.rs`,
     which links the crate directly rather than re-including headers).
- [ ] **Step 4: Run to verify it passes** — `cargo build -p fontconfig-sys` → GREEN;
  `cargo test -p fontconfig-sys` → GREEN.
- [ ] **Step 5: Commit** — `git commit -m "rustty: vendor fontconfig-sys (fontconfig
  2.14.2, cc+bindgen+KAT, deps freetype-sys+libxml2-sys) — R3 (L3 Task 3)"`

**Gate after Task 3:** `bash .port/tools/gates/check_acyclic.sh` green (`fontconfig-sys →
{freetype-sys, libxml2-sys}`, `libxml2-sys → {}`, no cycle); `cargo build --workspace`
unaffected (both crates are currently unreferenced by any `[dependencies]` table outside
`crates-sys/`, so this is inert until Task 4 wires `rustty-font-shaper` to
`fontconfig-sys`).

---

### Task 4: `crate::fc` — the Rust binding wrapper over `fontconfig-sys`

**Files:**
- Create: `crates/rustty-font-shaper/src/fc.rs`
- Modify: `crates/rustty-font-shaper/src/lib.rs` — `#[cfg(not(any(target_os = "macos",
  target_arch = "wasm32")))] pub mod fc;`
- Modify: `crates/rustty-font-shaper/Cargo.toml` — new
  `[target.'cfg(not(any(target_os = "macos", target_arch = "wasm32")))'.dependencies]`
  table: `fontconfig-sys = { path = "../../crates-sys/fontconfig-sys" }`

**Interfaces:**
- Consumes: `fontconfig_sys as c` (Task 3).
- Produces (faithful port of `pkg/fontconfig/*.zig`, sibling of `ft.rs`/`hb.rs`):
  - `fc::init() -> bool` (`FcInit`, `init.zig:5-7`), `fc::init_load_config_and_fonts() ->
    *mut Config` (`initLoadConfigAndFonts`, `init.zig:17-19`), `fc::version() -> u32`
    (`init.zig:21-23`).
  - `fc::Config` — opaque raw-pointer newtype (raw-ptr discipline: no borrow-checked
    lifetime, matching the reference's manually-managed `*Config`); `destroy(&mut self)`,
    `font_sort(&mut self, pat: &Pattern, trim: bool, charset: Option<*mut *mut
    CharSet>) -> FontSortResult` (`config.zig:20-36`), `font_render_prepare(&mut self, pat:
    &Pattern, font: &Pattern) -> Result<*mut Pattern, Error>` (`config.zig:38-43`),
    `substitute_with_pat(&mut self, pat: &Pattern, kind: MatchKind) -> bool`
    (`config.zig:45-51`).
  - `fc::Pattern` — opaque raw-ptr newtype; `create() -> *mut Pattern` (`pattern.zig:13-
    15`), `destroy(&mut self)`, `default_substitute(&mut self)`, `add(&mut self, prop:
    Property, value: Value, append: bool) -> bool` (`pattern.zig:29-36`), `get(&self, prop:
    Property, id: u32) -> Result<Value, Error>` (`pattern.zig:38-48`).
  - `fc::CharSet` — `create() -> *mut CharSet`, `destroy(&mut self)`, `add_char(&mut self,
    cp: u32) -> bool`, `has_char(&self, cp: u32) -> bool` (`char_set.zig:6-28`).
  - `fc::LangSet` — `create() -> *mut LangSet`, `destroy(&mut self)`, `add_lang(&mut self,
    lang: &CStr) -> bool`, `has_lang(&self, lang: &CStr) -> bool` (`lang_set.zig:5-28`).
  - `fc::FontSet` — `create()`/`destroy()`, `fonts(&self) -> &[*mut Pattern]`
    (`font_set.zig:6-34`).
  - `fc::{Weight, Slant, Spacing, Property, Result as FcResult, MatchKind}` enums
    (`common.zig:5-140`, including `Property::cval(self) -> &'static CStr` — the
    underscore-stripped property-name table, `common.zig:85-108`, and
    `Result::to_error(self) -> Result<(), Error>`, `common.zig:124-132`).
  - `fc::{Type, Value}` (`value.zig:9-70` — model as a Rust enum over the subset the
    discovery layer needs: `String(&CStr)`, `Integer(i32)`, `CharSet(*const CharSet)`,
    `LangSet(*const LangSet)`; extend if `toFcPattern`/`get` calls in Task 5/7 need more
    variants).
  - `fc::Error` (`error.zig:1-7`: `OutOfMemory`, `FontconfigFailed`,
    `FontconfigNoMatch`, `FontconfigTypeMismatch`, `FontconfigNoId`).

- [ ] **Step 1: Write the failing tests** — port all 11 `pkg/fontconfig` tests as
  `#[test]`s in `fc.rs`'s own `#[cfg(test)] mod tests`: `fc_version` (`init.zig:25-28`),
  `fc_init` (`init.zig:30-33`), `fc_init_load_config` (`init.zig:35-38`),
  `fc_init_load_config_and_fonts` (`init.zig:40-43`), `pattern_create`
  (`pattern.zig:147-161`), `pattern_name_parse` (`pattern.zig:163-168`), `property_cval`
  (`common.zig:110-114`), `char_set_create` (`char_set.zig:31-40`), `font_set_create`
  (`font_set.zig:37-44`), `lang_set_create` (`lang_set.zig:31-38`),
  `lang_set_has_lang_exact_match` (`lang_set.zig:40-61`).
- [ ] **Step 2: Run to verify they fail** — `cargo test -p rustty-font-shaper fc::`  →
  FAIL (module doesn't exist).
- [ ] **Step 3: Implement** — write `fc.rs` per the Interfaces section above, each method
  a thin `unsafe` call into `fontconfig_sys::Fc*`, matching `ft.rs`'s existing idiom
  (module doc comment explaining the "additive glue, not the full `pkg/fontconfig` test
  contract [wait — it IS the full contract here, all 11 tests ported], R1" framing);
  wire the crate dep + `pub mod fc;`.
- [ ] **Step 4: Run to verify they pass** — `cargo test -p rustty-font-shaper fc::` → 11
  tests PASS (exercising the REAL vendored fontconfig against a `FcInitLoadConfigAndFonts`
  call, which reads this host's `/etc/fonts/fonts.conf` — confirmed present via
  `fc-list`/`fc-match` on this box during planning).
- [ ] **Step 5: Commit** — `git commit -m "rustty-font-shaper: fc.rs — Fontconfig FFI
  wrapper (port of pkg/fontconfig/*.zig, 11 tests) (L3 Task 4)"`

---

### Task 5: `Descriptor::to_fc_pattern` (`toFcPattern`)

**Files:**
- Modify: `crates/rustty-font-shaper/src/discovery.rs:286-332` (replace the DEFERRED
  comment block with a real `impl Descriptor` method, `not(any(macos, wasm32))`-gated)

**Interfaces:**
- Consumes: `crate::fc::{Pattern, CharSet, Weight, Slant, Spacing, Property, Value}`
  (Task 4).
- Produces: `Descriptor::to_fc_pattern(&self) -> *mut fc::Pattern` — port of
  `discovery.zig:117-157` verbatim: `family`→`Property::Family`, `style`→
  `Property::Style`, `codepoint`→ a fresh `CharSet` with the one char added →
  `Property::Charset`, `size` (rounded) → `Property::Size`, `bold`→
  `Property::Weight`/`Weight::Bold`, `italic`→`Property::Slant`/`Slant::Italic`, and
  UNCONDITIONALLY `Property::Spacing`/`Spacing::Mono` (the reference always biases
  fontconfig toward monospace matches, `discovery.zig:147-154`). Each `pat.add` result is
  `assert`ed via `rustty_util::quirks::inlineAssert` (already imported elsewhere in this
  crate, e.g. `face/freetype.rs:41`), matching `discovery.zig`'s `assert(pat.add(...))`.

- [ ] **Step 1: Write the failing test** — `#[test] fn descriptor_to_fc_pattern_family_size_mono()`:
  build a `Descriptor { family: Some(b"monospace".to_vec()), size: 12.0, ..Default::default()
  }`, call `to_fc_pattern`, then `fc::Pattern::get(&pat, Property::Family, 0)` →
  `Value::String("monospace")`, `Property::Size, 0` → `Value::Integer(12)`,
  `Property::Spacing, 0` → `Value::Integer(Spacing::Mono as i32)` (always present, per the
  reference's unconditional add). `#[test] fn descriptor_to_fc_pattern_codepoint()`:
  `Descriptor { codepoint: u32::from('A'), ..Default::default() }` → `Property::Charset,
  0` → a `CharSet` whose `has_char('A')` is true.
- [ ] **Step 2: Run to verify they fail** — `cargo test -p rustty-font-shaper
  descriptor_to_fc_pattern` → FAIL (method doesn't exist).
- [ ] **Step 3: Implement** — per Interfaces above, from `discovery.zig:117-157`.
- [ ] **Step 4: Run to verify they pass** — same command → PASS.
- [ ] **Step 5: Commit** — `git commit -m "rustty-font-shaper: Descriptor::to_fc_pattern
  (toFcPattern) (L3 Task 5)"`

---

### Task 6: The real `Fontconfig` discoverer + `DiscoverIterator`

**Files:**
- Modify: `crates/rustty-font-shaper/src/discovery.rs` — replace Task 1's typed-stub
  `Fontconfig`/`DiscoverIterator`/`Error` with real bodies (the DEFERRED comment block at
  `discovery.rs:350-433` becomes the implementation); flesh out the 2 skip-stub `#[test]`s
  at `discovery.rs:1753-1792`.

**Interfaces:**
- Consumes: `crate::fc::*` (Task 4), `Descriptor::to_fc_pattern` (Task 5),
  `crate::library::Library` (= `FreetypeLibrary` on this target class).
- Produces (port of `discovery.zig:247-339`):
  - `Fontconfig { fc_config: *mut fc::Config }`; `init(lib: Library) -> Fontconfig` (`_ =
    lib` — unused, matches `discovery.zig:250-255`: `fc::init()` + `fc::
    init_load_config_and_fonts()`); `deinit(&mut self)` (`fc_config.destroy()`,
    `discovery.zig:257-259`).
  - `discover(&self, desc: Descriptor) -> Result<DiscoverIterator, Error>` — build the
    pattern via `desc.to_fc_pattern()`, `self.fc_config.substitute_with_pat(pat,
    MatchKind::Pattern)` (asserted), `pat.default_substitute()`, `self.fc_config.
    font_sort(pat, false, None)`; if `result != Result::Match` return
    `Error::FontConfigFailed` — **note the exact reference spelling**: this is the AD HOC
    `error.FontConfigFailed` at `discovery.zig:278` (capital "C" in "Config"), a DIFFERENT
    Zig error atom from `pkg/fontconfig/error.zig`'s `Error.FontconfigFailed` (lowercase
    "c") that `Pattern::get`/`Config::font_render_prepare` propagate — model these as TWO
    distinct `discovery::Error` variants (e.g. `SortFailed` for the ad hoc one, `Fc(fc::
    Error)` wrapping the pkg-level one via `From`), do not conflate them.
  - `DiscoverIterator { config: *mut fc::Config, pattern: *mut fc::Pattern, set: *mut fc::
    FontSet, fonts: Vec<*mut fc::Pattern> /* or a slice view */, variations: Vec<Variation>,
    i: usize }`; `deinit(&mut self)` (`set.destroy(); pattern.destroy();`,
    `discovery.zig:309-313`); `next(&mut self) -> Result<Option<DeferredFace>, Error>` —
    `self.config.font_render_prepare(self.pattern, self.fonts[self.i])`, build
    `DeferredFace { fc: Some(FontconfigData { pattern: font_pattern, charset:
    font_pattern.get(Property::Charset,0)?.as_char_set(), langset: font_pattern.
    get(Property::Lang,0)?.as_lang_set(), variations: self.variations.clone() }) }`,
    `self.i += 1` (`discovery.zig:315-337`).
  - `discover_fallback(&self, collection: *mut Collection, desc: Descriptor) ->
    Result<DiscoverIterator, Error>` — `_ = collection; self.discover(desc)`
    (`discovery.zig:291-299`).
  - `discovery::Error` fully fleshed (composing `Fc(fc::Error)`, `SortFailed`, plus a
    `Face(FaceError)`/`Collection(collection::Error)` arm so `shared_grid_set.rs:94/108`
    keeps compiling — mirror the macOS `coretext_impl::Error` shape,
    `discovery.rs:472-493`).

- [ ] **Step 1: Write the failing tests** — flesh out the 2 EXISTING skip-stub `#[test]`s
  (`discovery.rs:1753-1768` and `:1771-1792`) with real bodies, porting `discovery.zig`'s
  `"fontconfig"` test (:1161-1174: `Fontconfig::init`, `discover(Descriptor{family:
  Some("monospace"), size: 12.0, ..})`, assert it doesn't error) and `"fontconfig
  codepoint"` test (:1176-1198: `discover(Descriptor{codepoint: 'A', size: 12.0, ..})`,
  `it.next()` → `Some(face)`, `face.has_codepoint('A', None)` true, `face.
  has_codepoint('B', None)` true — this second assertion depends on Task 7's
  `has_codepoint` landing; if sequenced strictly, stub `has_codepoint` here and finish
  the assertion in Task 7's step, OR do Tasks 6+7 in one PR-sized unit if that reads
  cleaner during execution).
- [ ] **Step 2: Run to verify they fail** — `cargo test -p rustty-font-shaper fontconfig`
  → FAIL.
- [ ] **Step 3: Implement** — per Interfaces above, from `discovery.zig:247-339`.
- [ ] **Step 4: Run to verify they pass** — same command → PASS (discovers real fonts
  from this host's fontconfig config — verified reachable via `fc-match monospace` →
  `Noto Sans Mono` during planning).
- [ ] **Step 5: Commit** — `git commit -m "rustty-font-shaper: real Fontconfig discoverer
  + DiscoverIterator (L3 Task 6)"`

---

### Task 7: `DeferredFace` Fontconfig payload + the 3 `unimplemented!()` arms

**Files:**
- Modify: `crates/rustty-font-shaper/src/deferred_face.rs`

**Interfaces:**
- Consumes: `crate::fc::*` (Task 4), the real `discovery::Fontconfig::discover` (Task 6).
- Produces (port of `DeferredFace.zig:21-23,37-56,129,149-150,179-180,205-238,325-347`):
  - Replace the non-macOS `pub _deferred: ()` field (`deferred_face.rs:49-50`) with `pub
    fc: Option<FontconfigData>` (mirrors the macOS `pub ct: Option<CoreTextData>` shape,
    `deferred_face.rs:45-46`).
  - `struct FontconfigData { pattern: *mut fc::Pattern, charset: *const fc::CharSet,
    langset: *const fc::LangSet, variations: Vec<Variation> }` (`DeferredFace.zig:38-56`);
    `deinit(&mut self)` = `pattern.destroy()` (`:52-55`).
  - `DeferredFace::undefined() -> DeferredFace` → `DeferredFace { fc: None }` (was `{
    _deferred: () }`).
  - `deinit(&mut self)` — `if let Some(fc) = self.fc.as_mut() { fc.deinit(); }`
    (`:129`).
  - `family_name`/`name` fontconfig arms — `(try fc.pattern.get(.family, 0)).string`
    (`:149-150`) / `(try fc.pattern.get(.fullname, 0)).string` (`:179-180`); on the
    `not(any(macos, wasm32))` target class these REPLACE the current always-empty `Ok(&[])`
    fallback (`deferred_face.rs:249-272`'s CoreText-only body doesn't exist on this
    target — Task 1 already left `name`/`load`/`has_codepoint` as the crate-level
    `unimplemented!()` trio at `:479,482,489`; `familyName` has no existing Rust
    definition on this target at all yet — add it alongside `name`).
  - `load(&mut self, lib: Library, opts: Options) -> Result<Face, FaceError>` → the
    `loadFontconfig` body (`:223-238`): read `Property::File`/`Property::Index` off
    `fc.pattern`, `Face::init_file(lib, filename_as_cstr, face_index, opts)`,
    `face.set_variations(&fc.variations, opts)`. Replaces the `unimplemented!()` at
    `deferred_face.rs:489`.
  - `has_codepoint(&self, cp: u32, p: Option<Presentation>) -> bool` → the fontconfig arm
    (`:325-347`): `fc.charset.has_char(cp)` (false-short-circuit), then if `p.is_some()`
    compare against `fc.langset.has_lang(c"und-zsye")` → `Presentation::Emoji` else
    `Presentation::Text`. Replaces the `unimplemented!()` at `deferred_face.rs:479`.
  - Delete the now-superseded `unimplemented!()` at `deferred_face.rs:482` (`name`) by
    folding it into the `family_name`/`name` pair above.

- [ ] **Step 1: Write the failing test** — extend Task 6's `"fontconfig codepoint"`
  discovery test to also assert `face.has_codepoint('B', None)` (the second-codepoint
  check, `discovery.zig:1197`) if it was stubbed in Task 6; add a focused
  `#[test] fn deferred_face_fontconfig_name_and_load()` in `deferred_face.rs`: discover
  `monospace`, call `.name(&mut buf)` (non-empty), call
  `.load(lib, Options { size: DesiredSize { points: 12.0, ..default DPI }, freetype_load_
  flags: FREETYPE_LOAD_FLAGS_DEFAULT })`, assert `face.glyph_index(' ').is_some()`.
- [ ] **Step 2: Run to verify they fail** — `cargo test -p rustty-font-shaper
  deferred_face` → FAIL (`unimplemented!()` panics).
- [ ] **Step 3: Implement** — per Interfaces above.
- [ ] **Step 4: Run to verify they pass** — same command → PASS.
- [ ] **Step 5: Commit** — `git commit -m "rustty-font-shaper: DeferredFace Fontconfig
  payload + load/has_codepoint/name arms (L3 Task 7)"`

---

### Task 8: The end-to-end KAT — port `DeferredFace.zig`'s `"fontconfig"` test

**Files:**
- Modify: `crates/rustty-font-shaper/src/deferred_face.rs` (the `fontconfig` `#[test]`
  currently a no-op body at `:493-507`)

**Interfaces:**
- Consumes: everything from Tasks 1-7.
- Produces: the single test that proves the whole tier — port of `DeferredFace.zig:468-
  498` verbatim: `Library::init()`, `discovery::Fontconfig::init(lib)`, `discover
  (Descriptor{family: Some("monospace"), size: 12.0, ..})`, `it.next()` → a `DeferredFace`,
  `def.name(&mut buf)` → non-empty, `def.load(lib, Options{size:{points:12.0}, ..})` →
  `Face`, `face.glyph_index(u32::from(' ')).is_some()` → true.

- [ ] **Step 1: Write the failing test** — replace the no-op body at
  `deferred_face.rs:493-507` with the real port (drop the
  `if crate::options::BACKEND == crate::Backend::FontconfigFreetype {}` no-op guard's
  empty body for a real one; keep the guard itself since this remains a
  backend-conditional test mirroring the reference's `SkipZigTest` early-return).
- [ ] **Step 2: Run to verify it fails** — `cargo test -p rustty-font-shaper --
  deferred_face::tests::fontconfig` → this specific test should already PASS trivially
  before this task (empty body) — the meaningful check is that it FAILS if you
  temporarily break any of Tasks 1-7 (a sanity/regression check), then confirm it passes
  once real.
- [ ] **Step 3: Implement** — per Interfaces above.
- [ ] **Step 4: Run to verify it passes** — `cargo test -p rustty-font-shaper` → FULL
  suite GREEN on Linux, including this KAT, against the REAL system fontconfig
  (`/etc/fonts/fonts.conf`, confirmed resolving `monospace` → `Noto Sans Mono[wght].ttf`
  on this host via `fc-match` during planning) and the vendored FreeType (already wired).
- [ ] **Step 5: Commit** — `git commit -m "rustty-font-shaper: fontconfig KAT — discover
  + load + glyph_index end-to-end on Linux (L3 Task 8)"`

**Gate after Task 8:** a real Linux system font resolves through rustty's font stack,
end to end. The 3 `unimplemented!()` leaves this plan set out to remove
(`deferred_face.rs:479,482,489` as of plan-authoring) are gone.

---

### Task 9: Gate wiring — acyclic, test-counts, DEFERRALS

**Files:**
- Modify: `.port/tools/gates/check_test_counts.py` (add the `font_shaper_fc` row)
- Modify: `.port/phase-2/DEFERRALS.md` (mark the L3 entry RESOLVED)

**Interfaces:**
- Consumes: nothing new — this task only updates bookkeeping/gates.
- Produces: `check_test_counts.py`'s manifest gains a `font_shaper_fc` row mapping the 10
  `pkg/fontconfig/*.zig` files (Task 4's Interfaces list) to `[f"{RSFS}/fc.rs"]`, count
  11.

- [ ] **Step 0 (pre-flight, do NOT skip): the manifest's path constants are STALE.**
  `check_test_counts.py:24-92` hardcodes `GH*`/`RS*` as `/Users/gael/projects/ghostty/...`
  / `/Users/gael/projects/rustty/...` (an old macOS checkout layout) — NOT
  `/home/gael/projects/ghostty` / `/home/gael/projects/rustty-gtk` (this environment).
  There is no env-var override (`grep -n "os.environ\|getenv"` on the file returns
  nothing relevant). This is a PRE-EXISTING condition, not introduced by this plan.
  Before adding the `font_shaper_fc` row: (a) run the script once unmodified and confirm
  it is currently non-functional here (exit 3 "BLOCKED"/missing-file, or silently
  vacuous) on THIS host; (b) if some other in-flight change has already repointed these
  constants (e.g. to env-derived paths) by the time this task executes, follow that
  convention instead of the literal paths below; (c) if not, add the new row using
  whatever base constant is live at the time (do not invent a `GHPKG`/`pkg`-relative
  helper unless the file already has one), and flag the broader staleness to the user
  rather than silently "fixing" 90+ unrelated rows outside this plan's scope.

- [ ] **Step 1: Write the failing check** — run
  `python3 .port/tools/gates/check_test_counts.py` before the edit; confirm `fc.rs`'s 11
  tests are currently UNTRACKED (no manifest row references them, so the gate is silently
  blind to this file, not red — note this as the "failing" precondition rather than a
  literal red gate).
- [ ] **Step 2: N/A** (bookkeeping task, no red/green test cycle in the TDD sense) —
  instead, confirm `check_test_counts.py`'s existing rows for `font_shaper_discovery`
  (`discovery.zig` 8 tests) and `font_shaper_deferred_face` (`DeferredFace.zig` 2 tests)
  are UNCHANGED by this plan (the reference test COUNT didn't change — only the bodies
  went from skip-stubs to real; the gate is line-count/name-based, not pass/fail-based,
  so it was already green before this plan and stays green).
- [ ] **Step 3: Implement** — add the `font_shaper_fc` manifest row (Task 4's 11 tests);
  in `DEFERRALS.md`, edit the `## rustty-font-shaper — Linux fontconfig backend +
  web_canvas` section (`DEFERRALS.md:3946-3965`): strike the `deferred_face.rs:479/482/
  489` bullet, replace with "**RESOLVED (this plan's date).** Ported in L3
  (`docs/superpowers/plans/2026-07-17-L3-fontconfig-discovery.md`): the real Fontconfig
  discoverer (`discovery.rs`), the `fc.rs` FFI wrapper, and the `DeferredFace` `fc`
  payload all landed; see Tasks 1-8." Cross-reference the commit SHAs from Tasks 1-8.
- [ ] **Step 4: Run to verify** — `python3 .port/tools/gates/check_test_counts.py` →
  GREEN (still — the new row adds a passing entry, doesn't regress existing ones);
  `bash .port/tools/gates/check_acyclic.sh` → GREEN; `cargo build --workspace` → GREEN on
  Linux; `cargo test -p rustty-font-shaper -p fontconfig-sys -p libxml2-sys` → GREEN.
- [ ] **Step 5: Commit** — `git commit -m "rustty-font-shaper: L3 fontconfig discovery —
  gate wiring + DEFERRALS resolution (L3 Task 9)"`

**Final gate:** `cargo build --workspace` green on Linux; `cargo test -p
rustty-font-shaper -p fontconfig-sys -p libxml2-sys` green; `check_acyclic.sh` /
`check_test_counts.py` green; `DEFERRALS.md` updated; zero `unimplemented!()` left in
`deferred_face.rs`'s Fontconfig arms. **A real Linux system font resolves through
rustty's font stack.**

---

## Deferred (out of L3 scope, documented)

- **Windows FreeType-scan discoverer** (`freetype_windows` backend, `discovery.zig:900-
  1349` `Windows` struct) — Windows is not a rustty-gtk target; stays a DEFERRED comment
  block (`discovery.rs:1557+`).
- **`fontconfig`'s cache subsystem** (`fccache.c`'s on-disk `.cache-*` binary cache) — the
  reference vendors and compiles it (it's in the `srcs` list, Task 3), but this plan does
  not add any cache-directory provisioning/permissions handling beyond what
  `FcInitLoadConfigAndFonts` does by default; if the KAT (Task 8) is slow on a cold cache,
  note it but do not build custom cache-priming — out of scope.
  `FC_CACHEDIR=\"/var/cache/fontconfig\"` is compiled in verbatim (Task 3) matching the
  reference; if that directory is unwritable in the execution sandbox, fontconfig falls
  back to its own internal no-cache behavior (a fontconfig runtime concern, not a rustty
  port gap).
- **Emoji/color-font presentation nuance beyond `und-zsye`** — the reference's
  `has_codepoint` presentation check is exactly the `und-zsye` langset lookup
  (`DeferredFace.zig:336-342`); ported verbatim, no enhancement.
- **`rustty-config`'s `FreetypeLoadFlags` DI wiring** (the `config.rs`/`face/freetype.rs`
  placeholder noted at `face/mod.rs:104-119`, break #9) — untouched by this plan; Task 1
  only aliases `face::Options`/`FreetypeLoadFlags` to the EXISTING `freetype::` types, it
  does not change what those types contain.
- **GTK/apprt font-config UI** (font picker, live font reload on `fonts.conf` change) —
  L5+ concern, not L3.

## Self-review notes (spec coverage)

- Orchestrator's 3 named hard-compile-error symptoms (`face::Options` vs
  `freetype::Options`, `Fontconfig::init` missing, `Error` unresolved) → all three fixed
  in Task 1, each traced to its exact call site (`shared_grid_set.rs:94,108,623`) before
  prescribing the fix. ✓
  Real-verified: `shared_grid_set.rs:623` (`Discover::init`) and `:94,108`
  (`discovery::Error`) are genuinely unconditional call/type sites on the
  `not(macos)` target class today — confirmed by direct grep, not assumed.
- `-sys` template fidelity (freetype-sys/harfbuzz-sys exemplars) → Tasks 2-3 cite the
  exact `cc::Build`/`bindgen::Builder`/`links=`/KAT shape from both existing crates and
  the precedent `.port/phase-2/wf-freetype-sys-vendor.js` workflow (network-fetch,
  version-pin discipline, "if fetch fails STOP, do not substitute a version"). ✓
- `toFcPattern` + the 8 `discovery.zig` tests (2 fontconfig-specific) + the 2
  `DeferredFace.zig` tests → Tasks 5/6/8, each citing the exact reference line range. ✓
- The reference's OWN `pkg/fontconfig` binding package (11 tests) — not just the
  discovery-layer call sites — ported as its own module (`fc.rs`) with its own test
  contract, matching how `ft.rs`/`hb.rs` already exist as siblings for FreeType/HarfBuzz;
  this was NOT explicitly named in the orchestrator's file list but is required to
  implement `toFcPattern`/`Fontconfig` faithfully rather than inlining raw FFI calls
  throughout `discovery.rs`. → Task 4, with a manifest-gate addition in Task 9 so it's not
  a silent, untracked test surface. ✓
- The `error.FontConfigFailed` (ad hoc) vs `fontconfig.Error.FontconfigFailed` (pkg-level)
  spelling collision — a genuine Zig-error-atom subtlety that a naive port would conflate
  — called out explicitly in Task 6 so the implementer preserves two distinct variants. ✓
- R3 (vendor the same upstream, no system pkg-config) — Global Constraints + Tasks 2/3,
  despite the system having fontconfig 2.17/libxml2 2.12 available (confirmed via
  `pkg-config --modversion` during planning) — explicitly NOT used, matching ghostty's
  pinned 2.14.2/2.11.5. ✓
- Gate wiring (acyclic/test-counts/DEFERRALS) → Task 9, mirroring the L1 exemplar's Task 8
  pattern of a dedicated bookkeeping task before the final gate. ✓
