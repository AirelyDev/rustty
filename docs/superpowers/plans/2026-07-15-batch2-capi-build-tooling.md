# Batch 2 — C-API Build Tooling — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development / executing-plans. Steps use `- [ ]` checkboxes.

**Goal:** Port ghostty's C-API build tooling — the CMake wrapper for the VT library, the Doxygen C-API docs config, the Makefile, and the 3 `c-vt-cmake*` examples — onto rustty, delegating to `cargo` instead of `zig`.

**Architecture:** The CMake wrapper builds `rustty-vt-c` (`cargo build -p rustty-vt-c`) and exposes IMPORTED targets `rustty-vt` (shared) / `rustty-vt-static` over the flat `target/<profile>/librustty_vt_c.{dylib,so,a}` artifacts + the checked-in `include/rustty/` cbindgen headers. Doxygen generates C-API docs from `include/rustty/`. The 3 examples consume the CMake package via FetchContent.

**Tech Stack:** CMake ≥3.19, Doxygen, cargo, clang, Make.

## Global Constraints

- **Oracle (READ-ONLY):** `/Users/gael/projects/ghostty` @ 4749c4e.
- **NO commits** until user review.
- Rebrand map (same as Batch 1) plus build-tooling specifics:
  - CMake target/package `ghostty-vt` → `rustty-vt`; `GHOSTTY_STATIC` → `RUSTTY_STATIC`
  - artifact stem: cargo emits **`librustty_vt_c`** (underscore, `_c` suffix) — NOT `librustty-vt`
  - `<ghostty/vt.h>` → `<rustty/vt.h>`; `ghostty_*`/`Ghostty*`/`GHOSTTY_*` symbols → `rustty_*`/`Rustty*`/`RUSTTY_*`
  - Doxygen `libghostty` → `librustty`; `include/ghostty` → `include/rustty`; `dist/doxygen/ghostty.css` → `dist/doxygen/rustty.css`
- **Key Zig→cargo mechanics** (from recon):
  - `find_program(zig)` → `find_program(cargo)`
  - `zig build -Demit-lib-vt <flags>` → `cargo build -p rustty-vt-c <profile> <flags>` run with WORKING_DIRECTORY = repo root
  - output root `zig-out/{lib,bin,include}` → **flat `target/<profile>/`** (no bin/lib split) + checked-in `include/rustty/`
  - `-Doptimize=ReleaseFast` (Release build types) → `--release` (→ `target/release/`); Debug → no flag (→ `target/debug/`)
  - cargo produces **no versioned soname / symlinks** → drop the SONAME→REALNAME symlink install dance; install the flat lib
  - `-Dtarget=<zig triple>` → `--target <rust triple>` (needs a triple map); `--prefix <dir>` → `--target-dir <dir>`
- **Static link deps are discovered empirically** during Task 4 build-test (Rust std's real deps differ from Zig's; don't guess-copy the `ntdll;kernel32`/SIMD list — encode what the linker actually requires).
- **Prerequisites already present:** `include/rustty/vt.h` + `include/rustty/vt/*.h` (30 headers) exist; `target/release/librustty_vt_c.{a,dylib}` builds via `cargo build -p rustty-vt-c --release`.

---

### Task 1: CMakeLists.txt + dist/cmake/ (the cargo-delegation wrapper)

**Files:**
- Create: `CMakeLists.txt`, `dist/cmake/RusttyCargoCompiler.cmake`, `dist/cmake/rustty-vt-config.cmake.in`, `dist/cmake/README.md`
- Reference: `/Users/gael/projects/ghostty/{CMakeLists.txt,dist/cmake/*}`

**Interfaces:**
- Produces: CMake package `rustty-vt` with IMPORTED targets `rustty-vt` (shared) + `rustty-vt-static`, and function `rustty_vt_add_target(NAME <n> RUST_TARGET <triple> [CARGO_FLAGS ...])`. Task 4 consumes these.

- [ ] **Step 1: Port the header-comment usage doc** — rebrand the 67-line usage block (cargo commands, `rustty-vt`/`rustty-vt-static` targets, FetchContent from `github.com/AirelyDev/rustty`, `find_package(rustty-vt)`, `rustty_vt_add_target`, pointers to `dist/cmake/README.md` + the `c-vt-cmake*` examples).

- [ ] **Step 2: Project + options** — `cmake_minimum_required(VERSION 3.19)`; `project(rustty-vt VERSION 0.1.0 LANGUAGES C)`; option `RUSTTY_CARGO_BUILD_FLAGS`; build-type→profile: Release/MinSizeRel/RelWithDebInfo → `_RUSTTY_CARGO_PROFILE=release` + flag `--release`, Debug → `_RUSTTY_CARGO_PROFILE=debug` + no flag; `find_program(CARGO_EXECUTABLE cargo REQUIRED)`.

- [ ] **Step 3: Artifact filename computation** — `_RUSTTY_OUT_DIR = ${CMAKE_CURRENT_SOURCE_DIR}/target/${_RUSTTY_CARGO_PROFILE}` (flat). Per-platform (CMake-generic idiom, stem `librustty_vt_c`):
  - APPLE: shared `librustty_vt_c.dylib`; static `librustty_vt_c.a`
  - WIN32: shared `rustty_vt_c.dll`, implib `rustty_vt_c.dll.lib`; static `librustty_vt_c.a`
  - else: shared `librustty_vt_c.so`; static `librustty_vt_c.a`
  All paths flat under `_RUSTTY_OUT_DIR` (no `bin/`/`lib/` split). Header dir = `${CMAKE_CURRENT_SOURCE_DIR}/include` (checked-in `include/rustty/`), no `MAKE_DIRECTORY` needed but harmless to keep.

- [ ] **Step 4: The build invocation** — `add_custom_command(OUTPUT <shared> <static> COMMAND ${CARGO_EXECUTABLE} build -p rustty-vt-c ${_RUSTTY_PROFILE_FLAG} ${RUSTTY_CARGO_BUILD_FLAGS} WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR} USES_TERMINAL)`; `add_custom_target(cargo_build_lib_vt ALL DEPENDS <shared> <static>)`; `ADDITIONAL_CLEAN_FILES` → point at `${CMAKE_CURRENT_SOURCE_DIR}/target` (or omit — cargo clean is separate; prefer omitting to avoid nuking the whole target dir).

- [ ] **Step 5: IMPORTED targets** — `rustty-vt` SHARED IMPORTED GLOBAL (IMPORTED_LOCATION, INTERFACE_INCLUDE_DIRECTORIES=include; APPLE IMPORTED_SONAME=`@rpath/librustty_vt_c.dylib`; WIN32 IMPORTED_IMPLIB; else IMPORTED_SONAME=`librustty_vt_c.so`); `add_dependencies(rustty-vt cargo_build_lib_vt)`. `rustty-vt-static` STATIC IMPORTED GLOBAL (IMPORTED_LOCATION, include dirs, `INTERFACE_COMPILE_DEFINITIONS RUSTTY_STATIC`). Leave `INTERFACE_LINK_LIBRARIES` **empty/minimal for now** — Task 4 fills it from the real linker errors.

- [ ] **Step 6: Install rules** — `include(GNUInstallDirs)`. Install shared lib flat to `${CMAKE_INSTALL_LIBDIR}` (Windows: DLL→bin, implib→lib; **no `.pdb`, no soname symlinks**). Install static `TYPE LIB`. `install(DIRECTORY include/rustty DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})`.

- [ ] **Step 7: find_package config export** — `configure_package_config_file(dist/cmake/rustty-vt-config.cmake.in → rustty-vt-config.cmake INSTALL_DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/rustty-vt)`; `write_basic_package_version_file(... SameMajorVersion)`; install both to `.../cmake/rustty-vt`.

- [ ] **Step 8: rustty_vt_add_target() cross function** — `cmake_parse_arguments(PARSE_ARGV 0 _RVT "" "NAME;RUST_TARGET" "CARGO_FLAGS")`. `_prefix=${CMAKE_CURRENT_BINARY_DIR}/rustty-${NAME}`; build `cargo build -p rustty-vt-c --release --target ${RUST_TARGET} --target-dir ${_prefix} ${CARGO_FLAGS}`; outputs under `${_prefix}/${RUST_TARGET}/release/librustty_vt_c.{a,dylib,so,dll}` (flat per-triple). Create IMPORTED `rustty-vt-static-${NAME}` (+ `RUSTTY_STATIC`) and `rustty-vt-${NAME}` shared, mirroring Step 5.

- [ ] **Step 9: RusttyCargoCompiler.cmake** — port `GhosttyZigCompiler.cmake` → a helper that sets up a cross **C linker** for the final C example. rustty ships no zig, but zig-as-linker is available; write `provide rustty_cross_compiler(RUST_TARGET <triple>)` that (a) maps the rust triple → a `zig cc -target <zig triple>` wrapper if `zig` is found (writing the `zig-cc`/`zig-cxx` wrapper scripts + PARENT_SCOPE `CMAKE_C_COMPILER`/`CMAKE_SYSTEM_NAME` exactly as the oracle), else (b) errors with guidance to install a cross toolchain / `cargo-zigbuild`. Keep `include_guard(GLOBAL)` and the before-`project()` contract.

- [ ] **Step 10: rustty-vt-config.cmake.in** — rebrand the config template: `rustty-vt::rustty-vt` (shared) + `rustty-vt::rustty-vt-static`; `@RUSTTY_VT_REALNAME@`/`@RUSTTY_VT_STATIC_REALNAME@` placeholders (substituted from Step 3 vars — set them as CMake vars so `configure_package_config_file` fills them); APPLE @rpath + `INTERFACE_LINK_OPTIONS LINKER:-rpath`; the static transitive-deps comment → the Rust-std deps discovered in Task 4; `check_required_components(rustty-vt)`.

- [ ] **Step 11: dist/cmake/README.md** — rebrand from the oracle (cargo delegation, FetchContent, cross flow).

- [ ] **Step 12: Verify configure** — `cmake -B build-cmake -S . -Wno-dev` succeeds (finds cargo, defines targets, no config error). Deferred build test happens in Task 4 (the examples exercise the targets). Also `cmake --build build-cmake --target cargo_build_lib_vt` builds `librustty_vt_c.{a,dylib}` (idempotent — already built). Expected: PASS.

---

### Task 2: Doxyfile + DoxygenLayout.xml + dist/doxygen/

**Files:**
- Create: `Doxyfile`, `DoxygenLayout.xml`, `dist/doxygen/{rustty.css,stylesheet.css,favicon.png,mobile-nav.js,header.html,footer.html}`
- Reference: `/Users/gael/projects/ghostty/{Doxyfile,DoxygenLayout.xml,dist/doxygen/*}`

- [ ] **Step 1: Copy brand-neutral assets verbatim** — `stylesheet.css`, `mobile-nav.js`, `header.html`, `footer.html` (grep-confirmed no `ghostty` strings). Copy `favicon.png` for now (art asset; flag for a rustty favicon swap later).

- [ ] **Step 2: `dist/doxygen/rustty.css`** — copy `ghostty.css`, rebrand the header comment ("Rustty Doxygen Custom Stylesheet") and brand-color comments; keep or adjust the accent colors.

- [ ] **Step 3: `DoxygenLayout.xml`** — copy verbatim (brand-neutral; the navindex tab relabels "API Groups"/"Data Types"/"C Headers" are product-agnostic).

- [ ] **Step 4: `Doxyfile`** — port with rebrands: `PROJECT_NAME="librustty"`; `PROJECT_LOGO` → a rustty asset or empty; `INPUT=include/rustty`; `HTML_OUTPUT` → `doc/librustty` (strip `zig-out/`); `MAN_OUTPUT` → `doc/man` (strip `zig-out/`); `HTML_EXTRA_STYLESHEET=dist/doxygen/rustty.css`; `HTML_EXTRA_FILES=dist/doxygen/favicon.png dist/doxygen/mobile-nav.js`; `HTML_HEADER=dist/doxygen/header.html`; `LAYOUT_FILE=DoxygenLayout.xml`; keep `EXAMPLE_PATH=example`, `STRIP_FROM_INC_PATH=include`, `PREDEFINED=__wasm__`, and all the C-API/hiding settings.

- [ ] **Step 5: Verify** — `doxygen Doxyfile` runs and exits 0, emitting HTML under `doc/librustty` with no fatal warnings about missing INPUT (`include/rustty` exists). Add `doc/` to `.gitignore` (generated output). Expected: PASS.

---

### Task 3: Makefile

**Files:**
- Create: `Makefile`
- Reference: `/Users/gael/projects/ghostty/Makefile`

- [ ] **Step 1: Write a trivial Makefile** — `init` target prints "You probably want to run `cargo build` instead."; `clean` runs `cargo clean` and removes `dist/RusttyKit.xcframework` + `build-cmake` + `doc/`. Drop the `glad` OpenGL-loader target (not needed until a GL renderer). `.PHONY: init clean`.

- [ ] **Step 2: Verify** — `make init` prints the hint; `make -n clean` shows the right commands. Expected: PASS.

---

### Task 4: The 3 c-vt-cmake examples

**Files:**
- Create: `example/c-vt-cmake/{CMakeLists.txt,README.md,src/main.c}`, `example/c-vt-cmake-static/{...}`, `example/c-vt-cmake-cross/{...}`
- Reference: `/Users/gael/projects/ghostty/example/c-vt-cmake{,-static,-cross}/`

**Interfaces:**
- Consumes: Task 1's CMake package (`rustty-vt` / `rustty-vt-static` / `rustty_vt_add_target`).

- [ ] **Step 1: Port `src/main.c` for all 3** — symbol-swap (`#include <ghostty/vt.h>`→`<rustty/vt.h>`; `Ghostty*`→`Rustty*`; `GHOSTTY_*`→`RUSTTY_*`; `ghostty_*`→`rustty_*`). The static one keeps its " (static)!" string literal difference; cross's is identical to shared. Keep the prose ("CMake-built program") rebranded.

- [ ] **Step 2: `example/c-vt-cmake/CMakeLists.txt`** — `project(c-vt-cmake C)`; `FetchContent_Declare(rustty GIT_REPOSITORY https://github.com/AirelyDev/rustty.git GIT_TAG main)` + `FetchContent_MakeAvailable(rustty)`; `add_executable(c_vt_cmake src/main.c)`; `target_link_libraries(c_vt_cmake PRIVATE rustty-vt)`. README notes the local-checkout override `-DFETCHCONTENT_SOURCE_DIR_RUSTTY=../..`.

- [ ] **Step 3: `example/c-vt-cmake-static/CMakeLists.txt`** — same but set the simd-off cargo flag (if rustty-vt-c has a no-simd/`--no-default-features` equivalent — verify the crate's features; otherwise omit) via `set(RUSTTY_CARGO_BUILD_FLAGS ... CACHE ... FORCE)` before `FetchContent_MakeAvailable`, and `target_link_libraries(... PRIVATE rustty-vt-static)`.

- [ ] **Step 4: `example/c-vt-cmake-cross/CMakeLists.txt`** — before `project()`: derive `RUST_TARGET` from host (macOS/Windows→a linux triple, Linux→a windows triple; overridable `-DRUST_TARGET=`); `include(../../dist/cmake/RusttyCargoCompiler.cmake)` + `rustty_cross_compiler(RUST_TARGET ...)`; `project(c-vt-cmake-cross C CXX)`; FetchContent rustty; `rustty_vt_add_target(NAME cross RUST_TARGET ${RUST_TARGET})`; `target_link_libraries(... PRIVATE rustty-vt-static-cross)`.

- [ ] **Step 5: Port the 3 READMEs** — rebrand (cargo, `rustty-vt`/`-static`/`-cross` targets, FetchContent override var, cross verify step).

- [ ] **Step 6: Build-verify native shared** — from `example/c-vt-cmake/`: `cmake -B build -DFETCHCONTENT_SOURCE_DIR_RUSTTY=../.. && cmake --build build`, then run `./build/c_vt_cmake`. Expected: builds, runs, prints the formatted terminal output. **Parity check:** the output should match ghostty's same example — build+run the ghostty one (`zig` is available) OR compare against the analogous ported `c-vt-formatter` output. Encode the expected output.

- [ ] **Step 7: Build-verify native static + fix link deps** — from `example/c-vt-cmake-static/`: configure + build. If the link fails, read the undefined-symbol errors and add the required system libs to `rustty-vt-static`'s `INTERFACE_LINK_LIBRARIES` in `CMakeLists.txt` (Task 1 Step 5) + the config template (Task 1 Step 10) + the README's transitive-deps note. Re-build until it links and runs. Expected: builds, runs, prints " (static)!" variant.

- [ ] **Step 8: Cross example** — verify `cmake` **configure** succeeds (the cross-compiler helper resolves via `zig cc`, or errors cleanly if no cross toolchain). Full cross-build is gated on a cross toolchain + `rustup target add`; if unavailable locally, document that and confirm the configure/structure is correct. Note the limitation in the batch report.

---

## Self-Review (author)

- **Spec coverage:** Batch-2 spec items → Task 1 (CMakeLists + dist/cmake), Task 2 (Doxyfile + doxygen assets), Task 3 (Makefile), Task 4 (3 c-vt-cmake examples). All covered.
- **Empirical unknowns flagged, not guessed:** static link-libs (Task 4 Step 7), simd-off cargo flag existence (Task 4 Step 3 — verify the crate feature), cross-build toolchain (Task 4 Step 8). These are discovered by building, per R5 (no fake gates).
- **Verification is real:** every task ends in an actual build/run (cmake configure+build, doxygen run, make, example execution), enabled by the cmake+doxygen install.
- **Consistency:** artifact stem `librustty_vt_c` (underscore) used everywhere; `rustty-vt`/`rustty-vt-static` CMake target names consistent across CMakeLists, config template, and examples.
