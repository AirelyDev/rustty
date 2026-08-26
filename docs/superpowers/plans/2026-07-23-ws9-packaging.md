# WS9 Packaging — Self-Contained xcframework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the shipped `RusttyKit.xcframework` genuinely self-contained — vendor the full GNU gettext `intl/` runtime into `gettext-sys` so `_dgettext`/`_bindtextdomain`/`_textdomain` come from rustty's own archive, drop the Homebrew `-lintl` link from BOTH sites, replace xtask's filesystem-glob archive discovery with a deterministic set derived from `cargo build --message-format=json`, and add the `ranlib` normalization pass the oracle runs before `libtool` — proven by a hermetic build with Homebrew's libintl hidden.

**Architecture:** Three independent seams. (1) `crates-sys/gettext-sys` grows from a 6-TU `localename` slice to the oracle's full 30-TU libintl build (23 `intl/` core TUs incl. the checked-in Bison-generated `plural.c` + 7 gnulib-lib TUs), mirroring `ghostty/pkg/libintl/build.zig` exactly (same upstream gettext 0.24, same `-DHAVE_CONFIG_H -DLOCALEDIR=""` flags, same include-path layering); the archive it emits then carries the catalog API, so `crates/rustty-os/build.rs`'s Homebrew link-search and `macos/build-rustty.sh`'s `OTHER_LDFLAGS` `-lintl` both go away. (2) A new `xtask/src/archives.rs` module parses cargo's JSON message stream into an explicit `(staticlib, native archives)` set — mirroring `GhosttyLib.zig`'s enumerated `deps.add(lib)` list rather than `read_dir` — with a hard error when a declared `static=` lib cannot be located. (3) The same module ports `LibtoolStep.zig`'s `normalizeArchive` (`cp` + `/usr/bin/ranlib` per source archive) into the combine step.

**Tech Stack:** Rust 1.93.1 (pinned), cargo workspace at `/Users/gael/projects/rustty`, `cc` + `bindgen` for the vendored C, `serde_json` (already a workspace dep) for the cargo message stream, Xcode 26.5 (`xcodebuild`, `libtool`, `ranlib`, `nm`, `otool`, `ar`), ghostty oracle at `/Users/gael/projects/ghostty` (pinned `4749c4e`, reference-only), Fedora aarch64 VM reachable as `ssh rvm`.

## Global Constraints

(Inherited verbatim from `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`.)

- R1 full fidelity vs the ghostty oracle at `../ghostty` (pinned `4749c4e`); the oracle is reference-only — no build-time dependency on it (R3).
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty); `ghostty_*` C-ABI aliases stay.
- R4/R5: acyclic crate graph; gates (`.port/tools/gates/check_acyclic.sh`, A4 ABI gate, Miri lanes) are invariant — never weakened to pass.
- DG5: C-ABI panic policy is ABORT — no unwind across the ABI.
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned.
- Ledger discipline: strike this WS's entries in `.port/phase-2/DEFERRALS.md` / `.port/phase-2/INTEGRATION_PARITY.md` / `macos/MILESTONE.md` in the same branch.
- Commit style: `subsystem: summary (chunk-ref)`; branch per WS; merge to main only with both platforms green (`cargo build --workspace` on macOS AND `ssh rvm 'cd ~/projects/rustty && cargo build --workspace'`).

## File Structure

| File                                                                                      | Responsibility                                                                                                     |
| ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `crates-sys/gettext-sys/vendor/intl/**`                                                   | NEW — verbatim upstream gettext-0.24 `gettext-runtime/intl/` core (23 `.c` + 7 private `.h`)                       |
| `crates-sys/gettext-sys/vendor/intl/gnulib-lib/**`                                        | NEW location for the 7 gnulib `.c` + 15 `.h` (13 move here from the flat `vendor/`, 2 are new `glthread/` headers) |
| `crates-sys/gettext-sys/build.rs`                                                         | `SOURCES` 6 → 30 TUs; three include paths; archive renamed `rustty_intl`                                           |
| `crates-sys/gettext-sys/src/lib.rs`                                                       | adds the `catalog` FFI module (`bindtextdomain`/`textdomain`/`dgettext`) + its link KAT                            |
| `crates-sys/gettext-sys/README.md`                                                        | provenance rewrite: full libintl, not a slice                                                                      |
| `crates/rustty-os/build.rs`                                                               | DELETE the Homebrew `-L/opt/homebrew/opt/gettext/lib -lintl` block (review finding WS4-M1)                         |
| `crates/rustty-os/src/i18n.rs`                                                            | adds the `gettext_` link/round-trip KAT                                                                            |
| `xtask/Cargo.toml`                                                                        | adds `serde_json`                                                                                                  |
| `xtask/src/archives.rs`                                                                   | NEW — cargo-JSON archive-set parsing + `ranlib` normalization, with unit tests                                     |
| `xtask/src/main.rs`                                                                       | `xcframework()` uses the JSON set + normalization instead of the `read_dir` glob                                   |
| `macos/build-rustty.sh`                                                                   | DELETE `-L/opt/homebrew/opt/gettext/lib -lintl` from `OTHER_LDFLAGS`                                               |
| `.port/tools/gates/no_homebrew_gettext_gate.sh`                                           | NEW — the hermetic acceptance gate (hides Homebrew libintl, rebuilds, asserts symbols)                             |
| `.port/phase-2/INTEGRATION_PARITY.md`, `.port/phase-2/DEFERRALS.md`, `macos/MILESTONE.md` | ledger strikes                                                                                                     |

## Pre-verified facts the tasks rely on (researched 2026-07-28 — do NOT re-derive)

- **zlib deflate is ALREADY FIXED.** `crates-sys/zlib-sys/vendor/` contains `deflate.c`, `trees.c`, `compress.c` and `build.rs` `SOURCES` (lines 43-53) compiles all of them; `macos/build-rustty.sh:20-23` already documents `-lz` as dropped and `OTHER_LDFLAGS` (line 31) no longer contains it. §2.3's zlib half is stale — this WS only fixes the gettext half, and Task 8 corrects the §2.3 wording accordingly. **Do not re-vendor zlib.**
- **Oracle build recipe** (`ghostty/pkg/libintl/build.zig`): flags are exactly `-DHAVE_CONFIG_H` and `-DLOCALEDIR=""`; include paths are `pkg/libintl/` (the checked-in `config.h`/`libintl.h`/`libgnuintl.h`), then `gettext-runtime/intl`, then `gettext-runtime/intl/gnulib-lib`; `srcs` is the 23 `intl/` TUs + 7 `gnulib-lib/` TUs listed in Task 1. `osdep.c`/`printf.c` are commented out (non-macOS) — do NOT vendor them.
- **Upstream tarball**: `https://deps.files.ghostty.org/gettext-0.24.tar.gz` (the exact URL `ghostty/pkg/libintl/build.zig.zon` pins), SHA-256 `c918503d593d70daf4844d175a13d816afacb667c06fba1ec9dcd5002c1518b7`, 27367136 bytes. Verified reachable 2026-07-28. `gettext-runtime/intl/plural.c` ships **pre-generated** in the tarball — no Bison/yacc is needed at build time.
- **The already-vendored files are byte-identical to gettext-0.24.** All 13 current flat files under `crates-sys/gettext-sys/vendor/` (`arg-nonnull.h flexmember.h getlocalename_l-unsafe.c getlocalename_l-unsafe.h localename-environ.c localename-table.h localename-unsafe.c localename.h setlocale-lock.c setlocale_null-unlocked.c setlocale_null.c setlocale_null.h verify.h`) `cmp`-match `gettext-0.24/gettext-runtime/intl/gnulib-lib/<same name>`, and `config.h`/`libintl.h`/`libgnuintl.h` `cmp`-match `ghostty/pkg/libintl/`. The restructure in Task 1 is a pure move plus additions — no content changes.
- **The full 30-TU build compiles clean and links standalone.** Trial-verified 2026-07-28 with `clang -c -w -DHAVE_CONFIG_H -DLOCALEDIR='""' -I<tmpl> -I<intl> -I<intl/gnulib-lib>`: 30/30 objects, zero errors. `nm -g` on the resulting archive shows `T _dgettext`, `T _bindtextdomain`, `T _textdomain`, `T _ngettext`, `T _dcgettext`, `T _gettext` (from `intl-compat.c`) plus the `_libintl_*` primaries. `nm -u` shows only libc symbols, CoreFoundation (`_CFPreferencesCopyAppValue`, `_CFLocaleCopyPreferredLanguages`, …) and intra-archive references — **no iconv, no other external library.**
- **The exact header closure** (from `clang -MM` over all 30 TUs) is the file list given in Task 1. `localename-table.h` and `verify.h` are not in that closure but are already vendored and byte-identical; they move along with the rest (harmless, keeps the vendor set a superset of upstream's needs).
- **`crates/rustty-os` already depends on `gettext-sys`** (`crates/rustty-os/Cargo.toml:67`, `gettext-sys = { path = "../../crates-sys/gettext-sys" }`), so the vendored archive's link directives already propagate. No new dependency edge, no acyclic-graph change.
- **`crates/rustty-os/src/i18n.rs:271-277`** declares its own `extern "C" { fn bindtextdomain(...); fn textdomain(...); fn dgettext(...); }` block — these are exactly the three symbols the full vendor supplies. `rustty_build_config::i18n()` is `true` on macOS (`crates/rustty-build-config/src/build.rs:127-133`), so `i18n::gettext_` really calls `dgettext` at runtime.
- **The `-lintl` link sites are exactly two**: `crates/rustty-os/build.rs:49-67` (the WS4-added block — review finding **WS4-M1**; it hardcodes `/opt/homebrew/opt/gettext/lib`, so it also misses Intel Homebrew at `/usr/local`) and `macos/build-rustty.sh:31` `OTHER_LDFLAGS`. `.port/tools/macos-parity/menu-shortcut-parity.sh:51` and `.port/phase-0/harness/build_port_driver.sh:153` reference libintl only as _optional_ harness flags and are out of scope.
- **`msgfmt` is a build-time TOOL dependency, not a link dependency.** `xtask/src/res_misc.rs:388-402` spawns Homebrew's `msgfmt` to compile the 31 `po/*.po` catalogs, and `msgfmt_path()` (line 405-420) prepends `/opt/homebrew/opt/gettext/bin`. The oracle equally needs `msgfmt` at build time (`GhosttyI18n.zig`). WS9's claim is about the **linked product**, so the acceptance gate hides Homebrew's libintl _libraries_ while leaving `bin/msgfmt` functional — see Task 6.
- **Cargo JSON shape** (verified by running `cargo build -p zlib-sys -p gettext-sys --message-format=json`): `{"reason":"build-script-executed","package_id":"path+file:///…/crates-sys/zlib-sys#0.1.0","linked_libs":["static=z"],"linked_paths":["native=/…/target/debug/build/zlib-sys-38fc4eb3de9b7298/out"],…}`. `linked_libs` entries are the raw `cargo:rustc-link-lib` values (`static=z`, `framework=CoreFoundation`, `dylib=pthread`, or a bare name); `linked_paths` are the raw `cargo:rustc-link-search` values (`native=<dir>` or a bare dir). `compiler-artifact` messages carry `target.kind` and `filenames`.
- **`ranlib` behaviour** (verified): `/usr/bin/ar rcS lib.a p.o` produces an archive whose `ar -t` lists only `p.o`; after `cp` + `/usr/bin/ranlib`, `ar -t` lists `__.SYMDEF SORTED` first. That is the assertion Task 4 uses.
- **Homebrew layout** (verified on this host): `/opt/homebrew/opt/gettext -> ../Cellar/gettext/1.0`; `/opt/homebrew/lib/{libintl.dylib,libintl.a,libintl.8.dylib}` and `/opt/homebrew/include/libintl.h` are symlinks into that Cellar. The Cellar `lib` dir is user-writable (`gael:staff`, `drwxr-xr-x`). `msgfmt` links `libintl.8.dylib` by **absolute path**, so renaming only the linker-facing `libintl.dylib` symlink and `libintl.a` disables `-lintl` everywhere while keeping `msgfmt` runnable.
- **Existing xtask conventions**: unit tests live in `#[cfg(test)] mod tests` inside the module (`xtask/src/res_themes.rs:107`, `wasm_patch_growable_table.rs:165`). `serde_json = { version = "1", features = ["preserve_order"] }` is already declared at workspace `Cargo.toml:284`.
- Acyclic gate: `bash .port/tools/gates/check_acyclic.sh` from the repo root.

---

## Task 0: Branch

- [ ] **Step 0.1: Create the WS9 branch off up-to-date main**

Run:

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull && git checkout -b ws9-packaging
```

Expected: `Switched to a new branch 'ws9-packaging'`.

---

## Task 1: `gettext-sys` — vendor the FULL libintl (§2.3, gettext half)

**Files:**

- Create: `crates-sys/gettext-sys/vendor/intl/` (23 `.c` + 7 `.h`), `crates-sys/gettext-sys/vendor/intl/gnulib-lib/` (7 `.c` + 13 `.h`), `crates-sys/gettext-sys/vendor/intl/gnulib-lib/glthread/` (2 `.h`)
- Delete: the 13 flat files currently at `crates-sys/gettext-sys/vendor/` (they move into `vendor/intl/gnulib-lib/`)
- Modify: `crates-sys/gettext-sys/build.rs` (`SOURCES` const lines 32-42; the `cc::Build` block lines 56-70; the `build.compile` call and its comment lines 62-84)
- Modify: `crates-sys/gettext-sys/src/lib.rs` (append the `catalog` module + tests)
- Modify: `crates-sys/gettext-sys/README.md` (provenance rewrite)

**Interfaces:**

- Produces: the `gettext-sys` static archive is now named **`rustty_intl`** (was `rustty_gettext_localename`) and defines `_dgettext`, `_dcgettext`, `_ngettext`, `_dngettext`, `_dcngettext`, `_bindtextdomain`, `_bind_textdomain_codeset`, `_textdomain`, `_gettext` plus the `_libintl_*` primaries and the existing `_libintl_locale_name_canonicalize`. It still needs `framework=CoreFoundation`.
- Produces: `gettext_sys::catalog::{bindtextdomain, textdomain, dgettext}` — raw `extern "C"` re-exports (macOS only), used by Task 1's KAT. `crates/rustty-os/src/i18n.rs` keeps its OWN identical `extern "C"` block and is NOT changed to use these (YAGNI — the symbols are the same).
- Consumes: nothing from earlier tasks.

- [ ] **Step 1.1: Write the failing catalog link KAT**

Append to `crates-sys/gettext-sys/src/lib.rs` (after the existing `locale_name_canonicalize` function, before the existing `#[cfg(all(test, target_os = "macos"))] mod tests`):

```rust
/// The GNU gettext **message-catalog runtime** — the second half of libintl,
/// vendored in full as of WS9 (the reference builds the whole
/// `gettext-runtime/intl/` core for macOS; see `ghostty/pkg/libintl/build.zig`
/// `srcs`).
///
/// These are the exact three entry points `os/i18n.zig` uses
/// (`bindtextdomain` in `init`, `textdomain` in `initGlobalDomain`, `dgettext`
/// in `_`). They are the POSIX-spelled compatibility wrappers defined by the
/// vendored `intl-compat.c`, which forward to the `libintl_*` primaries.
///
/// `crates/rustty-os/src/i18n.rs` declares the same three symbols in its own
/// `extern "C"` block (the direct port of the Zig `extern fn` declarations);
/// this module exists so the vendoring crate itself proves, in its own test
/// suite, that the archive actually carries them.
#[cfg(target_os = "macos")]
pub mod catalog {
    use std::os::raw::c_char;

    unsafe extern "C" {
        /// Bind message domain `domainname` to directory `dirname`. Returns the
        /// currently-bound directory, or NULL on allocation failure.
        pub fn bindtextdomain(domainname: *const c_char, dirname: *const c_char) -> *const c_char;
        /// Set the global default message domain. Returns the current domain,
        /// or NULL on allocation failure.
        pub fn textdomain(domainname: *const c_char) -> *const c_char;
        /// Look `msgid` up in `domainname`'s catalog. With no catalog bound it
        /// returns `msgid` itself.
        pub fn dgettext(domainname: *const c_char, msgid: *const c_char) -> *const c_char;
    }
}

#[cfg(all(test, target_os = "macos"))]
mod catalog_tests {
    use super::catalog::{bindtextdomain, dgettext, textdomain};
    use std::ffi::CStr;

    /// Link + behavior KAT for the vendored message-catalog runtime (WS9).
    ///
    /// Proves the full `intl/` core compiled, linked, and runs from rustty's
    /// OWN archive — no Homebrew `-lintl`. `bindtextdomain` must echo back the
    /// directory it was handed, and `dgettext` on a domain with no `.mo`
    /// catalog must return the msgid pointer-equal (gettext's documented
    /// no-catalog behavior), which is exactly what `os/i18n.zig`'s `_` relies
    /// on today.
    #[test]
    fn catalog_api_links_and_round_trips() {
        let domain = c"io.airely.rustty.ws9-test";
        let dir = c"/nonexistent/ws9/locale";

        // SAFETY: both arguments are valid NUL-terminated C strings; gettext
        // copies them into its own bindings table.
        let bound = unsafe { bindtextdomain(domain.as_ptr(), dir.as_ptr()) };
        assert!(!bound.is_null(), "bindtextdomain returned NULL (OOM)");
        // SAFETY: gettext returns a pointer to its own NUL-terminated copy,
        // valid for the program lifetime.
        assert_eq!(unsafe { CStr::from_ptr(bound) }, dir);

        let msgid = c"ws9 untranslated probe";
        // SAFETY: both arguments are valid NUL-terminated C strings.
        let out = unsafe { dgettext(domain.as_ptr(), msgid.as_ptr()) };
        assert!(!out.is_null(), "dgettext returned NULL");
        // No catalog exists at that path, so gettext hands the msgid straight
        // back — pointer-identical, per the gettext contract.
        assert_eq!(out, msgid.as_ptr());

        // Force `textdomain` to be linked too (the third symbol
        // `os/i18n.zig` needs) without mutating this process's global default
        // domain, which other tests in the crate could observe.
        let f: unsafe extern "C" fn(*const std::os::raw::c_char) -> *const std::os::raw::c_char =
            textdomain;
        assert!(!(f as *const ()).is_null());
    }
}
```

- [ ] **Step 1.2: Run the test to verify it fails**

Run: `cargo test -p gettext-sys --test-threads=1 2>&1 | tail -30`
(If that flag form is rejected, use `cargo test -p gettext-sys -- --test-threads=1`.)
Expected: **FAIL at link time**, with `Undefined symbols for architecture arm64: "_bindtextdomain", "_dgettext", "_textdomain"` (the vendored slice defines none of them, and `gettext-sys` itself does not link Homebrew).

- [ ] **Step 1.3: Download and verify the upstream gettext tarball**

Run:

```bash
cd /Users/gael/projects/rustty
mkdir -p /tmp/ws9 && cd /tmp/ws9
curl -sSL -o gettext-0.24.tar.gz https://deps.files.ghostty.org/gettext-0.24.tar.gz
shasum -a 256 gettext-0.24.tar.gz
```

Expected: `c918503d593d70daf4844d175a13d816afacb667c06fba1ec9dcd5002c1518b7  gettext-0.24.tar.gz`.
**If the hash differs, STOP** — do not vendor an unverified tarball; report the mismatch.

Then extract only the `intl/` subtree:

```bash
cd /tmp/ws9 && tar xzf gettext-0.24.tar.gz gettext-0.24/gettext-runtime/intl
ls /tmp/ws9/gettext-0.24/gettext-runtime/intl/plural.c
```

Expected: the `plural.c` path prints (it ships pre-generated; no Bison needed).

- [ ] **Step 1.4: Restructure `vendor/` and copy the full source set**

Run (exact, from the repo root):

```bash
cd /Users/gael/projects/rustty
U=/tmp/ws9/gettext-0.24/gettext-runtime/intl
V=crates-sys/gettext-sys/vendor

# 1. The 13 flat gnulib files move into their upstream-shaped home.
git rm -q $V/arg-nonnull.h $V/flexmember.h $V/getlocalename_l-unsafe.c \
  $V/getlocalename_l-unsafe.h $V/localename-environ.c $V/localename-table.h \
  $V/localename-unsafe.c $V/localename.h $V/setlocale-lock.c \
  $V/setlocale_null-unlocked.c $V/setlocale_null.c $V/setlocale_null.h $V/verify.h

mkdir -p $V/intl/gnulib-lib/glthread

# 2. The intl/ core: 23 translation units.
for f in bindtextdom.c compat.c dcgettext.c dcigettext.c dcngettext.c dgettext.c \
         dngettext.c explodename.c finddomain.c gettext.c hash-string.c \
         intl-compat.c l10nflist.c langprefs.c loadmsgcat.c localealias.c log.c \
         ngettext.c plural-exp.c plural.c setlocale.c textdomain.c version.c; do
  cp "$U/$f" "$V/intl/$f"
done

# 3. The intl/ private headers (the clang -MM closure).
for f in eval-plural.h gettextP.h gmo.h hash-string.h loadinfo.h plural-exp.h plural.h; do
  cp "$U/$f" "$V/intl/$f"
done

# 4. The gnulib-lib translation units: 7 (the 6 previously vendored + localename.c).
for f in getlocalename_l-unsafe.c localename.c localename-environ.c \
         localename-unsafe.c setlocale-lock.c setlocale_null.c \
         setlocale_null-unlocked.c; do
  cp "$U/gnulib-lib/$f" "$V/intl/gnulib-lib/$f"
done

# 5. The gnulib-lib headers (closure + the two already-vendored extras).
for f in arg-nonnull.h attribute.h filename.h flexmember.h \
         getlocalename_l-unsafe.h localcharset.h localename.h localename-table.h \
         setlocale-messages.h setlocale_null.h struniq.h thread-optim.h verify.h; do
  cp "$U/gnulib-lib/$f" "$V/intl/gnulib-lib/$f"
done
for f in lock.h once.h; do
  cp "$U/gnulib-lib/glthread/$f" "$V/intl/gnulib-lib/glthread/$f"
done
```

- [ ] **Step 1.5: Verify the vendored tree byte-matches upstream and has the right shape**

Run:

```bash
cd /Users/gael/projects/rustty
U=/tmp/ws9/gettext-0.24/gettext-runtime/intl
V=crates-sys/gettext-sys/vendor
( cd "$V/intl" && find . -type f | sort ) | while read -r f; do
  cmp -s "$V/intl/$f" "$U/$f" || echo "MISMATCH $f"
done
echo "intl TUs:        $(ls $V/intl/*.c | wc -l | tr -d ' ')"
echo "gnulib TUs:      $(ls $V/intl/gnulib-lib/*.c | wc -l | tr -d ' ')"
echo "flat leftovers:  $(ls $V/*.c 2>/dev/null | wc -l | tr -d ' ')"
```

Expected: no `MISMATCH` lines; `intl TUs: 23`; `gnulib TUs: 7`; `flat leftovers: 0`. The four remaining top-level files are `config.h`, `libintl.h`, `libgnuintl.h`, `wrapper.h`.

- [ ] **Step 1.6: Update `crates-sys/gettext-sys/build.rs`**

Replace the `SOURCES` const (currently lines 32-42, from the `/// Vendored gnulib …` doc comment through the closing `];`) with:

```rust
/// Vendored gettext translation units compiled into this crate.
///
/// This is the FULL macOS libintl the reference builds — the exact `srcs` list
/// of `ghostty/pkg/libintl/build.zig`, in the same order: the 23
/// `gettext-runtime/intl/` core units (message catalogs, domain bindings, the
/// plural-form parser) plus the 7 `gnulib-lib/` units macOS needs. The
/// reference's commented-out `osdep.c`/`printf.c` are deliberately NOT here —
/// they are for platforms the reference does not build libintl for.
///
/// `plural.c` is the checked-in Bison-generated parser that ships pre-generated
/// in the upstream tarball; no yacc/bison runs at build time.
const SOURCES: &[&str] = &[
    "vendor/intl/bindtextdom.c",
    "vendor/intl/dcgettext.c",
    "vendor/intl/dcigettext.c",
    "vendor/intl/dcngettext.c",
    "vendor/intl/dgettext.c",
    "vendor/intl/dngettext.c",
    "vendor/intl/explodename.c",
    "vendor/intl/finddomain.c",
    "vendor/intl/gettext.c",
    "vendor/intl/hash-string.c",
    "vendor/intl/intl-compat.c",
    "vendor/intl/l10nflist.c",
    "vendor/intl/langprefs.c",
    "vendor/intl/loadmsgcat.c",
    "vendor/intl/localealias.c",
    "vendor/intl/log.c",
    "vendor/intl/ngettext.c",
    "vendor/intl/plural-exp.c",
    "vendor/intl/plural.c",
    "vendor/intl/setlocale.c",
    "vendor/intl/textdomain.c",
    "vendor/intl/version.c",
    "vendor/intl/compat.c",
    "vendor/intl/gnulib-lib/getlocalename_l-unsafe.c",
    "vendor/intl/gnulib-lib/localename.c",
    "vendor/intl/gnulib-lib/localename-environ.c",
    "vendor/intl/gnulib-lib/localename-unsafe.c",
    "vendor/intl/gnulib-lib/setlocale-lock.c",
    "vendor/intl/gnulib-lib/setlocale_null.c",
    "vendor/intl/gnulib-lib/setlocale_null-unlocked.c",
];
```

Then replace the `cc::Build` configuration block (currently lines 56-70, from `let mut build = cc::Build::new();` through the `for src in SOURCES { … }` loop) with:

```rust
    let mut build = cc::Build::new();
    build
        // Include-path layering mirrors the reference exactly
        // (pkg/libintl/build.zig): first the checked-in build template
        // (config.h, libintl.h, libgnuintl.h), then the upstream `intl/`
        // sources, then `intl/gnulib-lib/`.
        .include(&vendor)
        .include(vendor.join("intl"))
        .include(vendor.join("intl/gnulib-lib"))
        // Match the reference's flags (pkg/libintl/build.zig): config.h-driven
        // build with an empty LOCALEDIR. No other -D is passed there and none
        // is needed here — the full 30-TU set compiles clean with just these.
        .define("HAVE_CONFIG_H", None)
        .define("LOCALEDIR", Some("\"\""))
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w");
    for src in SOURCES {
        build.file(manifest_dir.join(src));
        println!("cargo:rerun-if-changed={src}");
    }
```

Then replace the `build.compile(...)` call and its preceding comment (currently lines 62-84, the long `WS4 NOTE` block ending in `build.compile("rustty_gettext_localename");`) with:

```rust
    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=...` + search-path directives. Because the
    // crate declares `links = "intl"`, Cargo enforces a single linkage.
    //
    // The archive is named `rustty_intl`, NOT `intl`. That is deliberate: the
    // `links` key stays "intl" (the pkg-config-style uniqueness tag for the
    // underlying C library concept), while the ARCHIVE name is namespaced so it
    // can never be confused with — or silently shadowed by — a system/Homebrew
    // `libintl`. As of WS9 nothing in the workspace passes `-lintl` any more:
    // this archive is the full libintl (catalog API included), so
    // `rustty-os/build.rs`'s Homebrew link-search and `macos/build-rustty.sh`'s
    // `OTHER_LDFLAGS -lintl` were both removed.
    build.compile("rustty_intl");
```

Finally, extend the `rerun-if-changed` tail (currently the last three `println!` lines of `main`) to cover the moved headers:

```rust
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=vendor/wrapper.h");
    println!("cargo:rerun-if-changed=vendor/config.h");
    println!("cargo:rerun-if-changed=vendor/libintl.h");
    println!("cargo:rerun-if-changed=vendor/libgnuintl.h");
    println!("cargo:rerun-if-changed=vendor/intl");
```

- [ ] **Step 1.7: Update the module-level doc comment in `build.rs`**

Replace the first doc line (`//! PORT STATUS: vendor slice (macOS `_libintl_locale_name_canonicalize`)`) with:

```rust
//! PORT STATUS: complete (full macOS libintl — catalog API + localename)
```

and replace the `//! 1.` bullet's parenthetical `(the gnulib-lib `.c`/`.h` come from the gettext upstream (gettext-runtime/intl/gnulib-lib)` clause so the whole bullet reads:

```rust
//!   1. The upstream C sources for `intl` are vendored in-tree under
//!      `vendor/intl/` (copied verbatim from gettext 0.24; this build NEVER
//!      references /Users/gael/projects/ghostty). The layout mirrors upstream:
//!      `vendor/intl/*.c` is the gettext-runtime `intl/` core and
//!      `vendor/intl/gnulib-lib/*.c` the gnulib support units. The build
//!      template (config.h, libgnuintl.h, libintl.h) stays at `vendor/` and is
//!      copied from ghostty/pkg/libintl, which is the reference's macOS libintl
//!      build.
```

- [ ] **Step 1.8: Run the test to verify it passes**

Run: `cargo test -p gettext-sys 2>&1 | tail -20`
Expected: PASS — `test catalog_tests::catalog_api_links_and_round_trips ... ok` and the pre-existing `tests::canonicalize_known_answers ... ok`; `test result: ok. 2 passed`.

- [ ] **Step 1.9: Assert the symbols at the archive level**

Run:

```bash
cd /Users/gael/projects/rustty
ARCHIVE=$(ls -t target/debug/build/gettext-sys-*/out/librustty_intl.a | head -1)
echo "archive: $ARCHIVE"
nm -g "$ARCHIVE" | grep -E " T _(dgettext|bindtextdomain|textdomain)$" | sort
nm -g "$ARCHIVE" | grep -c " T _libintl_locale_name_canonicalize"
```

Expected: exactly three lines — `T _bindtextdomain`, `T _dgettext`, `T _textdomain` — and the count `1` (the pre-existing canonicalizer is still there).

- [ ] **Step 1.10: Rewrite `crates-sys/gettext-sys/README.md`**

Replace the entire file with:

```markdown
# Vendored GNU gettext C sources (R3)

This directory vendors C source copied from the **same** upstream library the
reference (ghostty) links for macOS i18n: **GNU gettext 0.24**. The reference
builds libintl from gettext **only on macOS** — Linux provides libintl in libc
— see `/Users/gael/projects/ghostty/pkg/libintl/build.zig`.

Per `.port/PORT_RULES.md` R3 the port is **self-contained**: these sources are
copied in-tree and built standalone with the `cc` crate (see `../build.rs`). The
crate has **zero** build- or run-time dependency on the ghostty tree; the
reference is an external oracle only.

## Scope: the FULL macOS libintl (WS9)

As of WS9 this vendors the reference's **entire** `srcs` list — 30 translation
units — not a slice. That means both halves of libintl:

- the **message-catalog runtime**: `dgettext` / `dcgettext` / `ngettext` /
  `bindtextdomain` / `textdomain` / … (`vendor/intl/*.c`), consumed by
  `crates/rustty-os/src/i18n.rs` (the port of `os/i18n.zig`); and
- the **gnulib localename** family (`vendor/intl/gnulib-lib/*.c`), which defines
  `gl_locale_name_canonicalize`, renamed to
  `_libintl_locale_name_canonicalize` by `config.h` and consumed by
  `canonicalizeLocale`.

Because the emitted archive carries the catalog API, the macOS app no longer
links Homebrew's gettext: `-L/opt/homebrew/opt/gettext/lib -lintl` was removed
from **both** `crates/rustty-os/build.rs` and `macos/build-rustty.sh`, and
xtask folds this archive into `libRusttyKit.a`. The hermetic proof lives at
`.port/tools/gates/no_homebrew_gettext_gate.sh`.

`vendor/intl/plural.c` is the Bison-generated plural-form parser. It ships
**pre-generated** in the upstream tarball, so no bison/yacc runs at build time.

The tests in `../src/lib.rs` compile the vendored C, link it (with Apple
CoreFoundation, which the macOS canonicalizer consults via CFPreferences for
legacy locale names), and exercise both halves with known answers — proving the
end-to-end R3 toolchain (copied C -> `cc` compile -> `bindgen` FFI ->
`links = "intl"` -> Rust test linking and running the symbols).

## Provenance

- Build template — copied verbatim from `ghostty/pkg/libintl/` (the reference's
  macOS libintl build), kept at the top of `vendor/`:
  - `config.h` — gettext's macOS `config.h` (generated on a Mac by the
    reference; includes the reference's manual `xlocale.h`
    addition at the end). Drives `-DHAVE_CONFIG_H`.
  - `libintl.h` — the public libintl header.
  - `libgnuintl.h` — `#include "libintl.h"`.
  - `wrapper.h` — declaration-only header for `bindgen`.
- Upstream sources — copied verbatim from
  `gettext-0.24/gettext-runtime/intl/`, preserving the upstream directory shape:
  - `vendor/intl/*.c`, `vendor/intl/*.h`
  - `vendor/intl/gnulib-lib/*.c`, `*.h`, `glthread/*.h`

  Tarball: `https://deps.files.ghostty.org/gettext-0.24.tar.gz` — the exact URL
  and version `ghostty/pkg/libintl/build.zig.zon` pins. SHA-256
  `c918503d593d70daf4844d175a13d816afacb667c06fba1ec9dcd5002c1518b7`.

## Upgrading

Re-download the tarball, re-copy the files listed in `build.rs`'s `SOURCES`
plus their header closure, and re-check `config.h` against the reference's
(`ghostty/pkg/libintl/config.h`) — in particular the manual `xlocale.h` include
the reference appends at the end, which is easy to lose on a regenerate.
```

- [ ] **Step 1.11: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates-sys/gettext-sys
git commit -m "gettext-sys: vendor the full macOS libintl (catalog API + localename) (WS9 T1)"
```

---

## Task 2: Drop the Homebrew `-lintl` from `rustty-os/build.rs` (review finding WS4-M1)

**Files:**

- Modify: `crates/rustty-os/build.rs:49-67` (delete the WS4 Homebrew block)
- Test: `crates/rustty-os/src/i18n.rs` (new test in the existing `#[cfg(test)] mod tests`, after `canonicalizeLocale_darwin`)

**Interfaces:**

- Consumes: the `rustty_intl` archive from Task 1 (defines `_dgettext`/`_bindtextdomain`/`_textdomain`), reaching `rustty-os` through the existing `gettext-sys` dependency at `crates/rustty-os/Cargo.toml:67`.
- Produces: no workspace build script references `/opt/homebrew` any more. Later tasks rely on this: Task 5's `build-rustty.sh` edit and Task 6's hermetic gate both assume `cargo build` itself is Homebrew-free.

- [ ] **Step 2.1: Write the failing test**

Add to `crates/rustty-os/src/i18n.rs`, inside the existing `#[cfg(test)] mod tests` block, after `canonicalizeLocale_darwin`:

```rust
    /// WS9 regression: `gettext_` resolves `dgettext` from rustty's OWN
    /// vendored libintl (`gettext-sys`), not from Homebrew.
    ///
    /// The build-script link-search that used to point at
    /// `/opt/homebrew/opt/gettext/lib` is gone, so this test only links if the
    /// vendored archive carries the catalog API. With no `.mo` catalog bound
    /// for the bundle-id domain, gettext returns the msgid unchanged — that is
    /// the exact behavior the reference's `_` has today.
    #[test]
    #[cfg_attr(miri, ignore)] // ffi: vendored gettext catalog runtime crosses the C boundary (ASan, not Miri)
    fn gettext_resolves_from_vendored_libintl() {
        if !cfg!(target_os = "macos") {
            return; // the vendored libintl is macOS-only (Linux gets it from libc)
        }
        let msgid = c"ws9 untranslated probe";
        assert_eq!(gettext_(msgid), msgid);
    }
```

- [ ] **Step 2.2: Run the test to verify it passes only via the vendored archive**

First confirm it currently passes _while still linking Homebrew_ (baseline), then remove the Homebrew block and re-run — the removal is the change under test.

Run: `cargo test -p rustty-os i18n:: 2>&1 | tail -20`
Expected: PASS (baseline — the symbols currently come from Homebrew).

- [ ] **Step 2.3: Delete the Homebrew link block**

In `crates/rustty-os/build.rs`, delete lines 49-67 in full — the `// WS4: …` comment block AND the `if target_os == "macos" { … }` body containing:

```rust
        println!("cargo:rustc-link-search=native=/opt/homebrew/opt/gettext/lib");
        println!("cargo:rustc-link-lib=dylib=intl");
```

Replace them with this comment (so the resolved history is legible and nobody re-adds it):

```rust
    // i18n / gettext: NOTHING to emit here.
    //
    // `crate::i18n` declares `extern "C" { fn bindtextdomain/textdomain/
    // dgettext(...) }` (the gettext message-catalog runtime). Those symbols
    // come from the `gettext-sys` dependency, which as of WS9 vendors the FULL
    // macOS libintl (the reference's whole `pkg/libintl/build.zig` `srcs` set)
    // and emits `cargo:rustc-link-lib=static=rustty_intl`.
    //
    // WS4 temporarily resolved them from Homebrew with
    // `-L/opt/homebrew/opt/gettext/lib -lintl`. That was a hard host
    // dependency on ARM Homebrew for EVERY macOS build (and silently wrong on
    // Intel Homebrew, which lives under /usr/local) — review finding WS4-M1,
    // RESOLVED by WS9. Do not reintroduce it: `.port/tools/gates/
    // no_homebrew_gettext_gate.sh` fails the build if anything does.
```

- [ ] **Step 2.4: Run the test again to verify it still passes without Homebrew in the link**

Run:

```bash
cd /Users/gael/projects/rustty
touch crates/rustty-os/build.rs
cargo test -p rustty-os i18n:: 2>&1 | tail -20
```

Expected: PASS — `gettext_resolves_from_vendored_libintl ... ok` and `canonicalizeLocale_darwin ... ok`.

- [ ] **Step 2.5: Assert no build script references Homebrew any more**

Run:

```bash
cd /Users/gael/projects/rustty
grep -rn "opt/homebrew\|/usr/local/opt" crates/*/build.rs crates-sys/*/build.rs || echo "CLEAN"
```

Expected: `CLEAN`.

- [ ] **Step 2.6: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-os/build.rs crates/rustty-os/src/i18n.rs
git commit -m "os: drop the Homebrew -lintl link-search; libintl now comes from gettext-sys (WS9 T2, resolves WS4-M1)"
```

---

## Task 3: `xtask::archives` — deterministic archive set from cargo JSON (§2.4)

**Files:**

- Create: `xtask/src/archives.rs`
- Modify: `xtask/Cargo.toml` (add `serde_json` to `[dependencies]`)
- Modify: `xtask/src/main.rs` (add `mod archives;` next to the other `mod` declarations near line 11-23)

**Interfaces:**

- Produces (Task 5 consumes these exact names):
  - `pub struct NativeArchiveRequest { pub package_id: String, pub lib_name: String, pub search_paths: Vec<PathBuf> }`
  - `pub struct ParsedArtifacts { pub staticlibs: Vec<PathBuf>, pub native: Vec<NativeArchiveRequest> }`
  - `pub fn parse_artifacts(json: &str) -> Result<ParsedArtifacts, String>`
  - `impl NativeArchiveRequest { pub fn resolve_with(&self, exists: &dyn Fn(&Path) -> bool) -> Result<PathBuf, String>; pub fn resolve(&self) -> Result<PathBuf, String> }`
- Consumes: nothing from earlier tasks.

- [ ] **Step 3.1: Add `serde_json` to `xtask/Cargo.toml`**

In `xtask/Cargo.toml`, at the end of the `[dependencies]` section (after the `rustty-terminfo = { workspace = true }` line):

```toml
# WS9: parse `cargo build --message-format=json` so the xcframework combine step
# folds an EXPLICIT, cargo-declared archive set (the port of GhosttyLib.zig's
# enumerated `deps.add(lib)` list) instead of globbing the target dir.
serde_json = { workspace = true }
```

- [ ] **Step 3.2: Declare the module in `xtask/src/main.rs`**

Add, immediately after the existing `mod dist;` line (~line 13):

```rust
mod archives;
```

- [ ] **Step 3.3: Write the failing tests**

Create `xtask/src/archives.rs` containing ONLY this test module for now (the impl comes in Step 3.5):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A realistic two-line slice of `cargo build --message-format=json`: one
    /// `compiler-artifact` for the staticlib target, one `build-script-executed`
    /// for a `-sys` crate. Shapes verified against a live cargo run (2026-07-28).
    const SAMPLE: &str = r#"{"reason":"compiler-artifact","package_id":"path+file:///w/crates/rustty-apprt-c#0.1.0","target":{"kind":["staticlib"],"crate_types":["staticlib"],"name":"rustty_apprt_c"},"filenames":["/w/target/aarch64-apple-darwin/xcframework/librustty_apprt_c.a"],"fresh":false}
{"reason":"build-script-executed","package_id":"path+file:///w/crates-sys/zlib-sys#0.1.0","linked_libs":["static=z"],"linked_paths":["native=/w/target/aarch64-apple-darwin/xcframework/build/zlib-sys-abc/out"],"cfgs":[],"env":[],"out_dir":"/w/target/aarch64-apple-darwin/xcframework/build/zlib-sys-abc/out"}"#;

    #[test]
    fn parses_staticlib_and_one_native_archive() {
        let p = parse_artifacts(SAMPLE).unwrap();
        assert_eq!(
            p.staticlibs,
            vec![PathBuf::from("/w/target/aarch64-apple-darwin/xcframework/librustty_apprt_c.a")]
        );
        assert_eq!(p.native.len(), 1);
        assert_eq!(p.native[0].lib_name, "z");
        assert_eq!(
            p.native[0].search_paths,
            vec![PathBuf::from(
                "/w/target/aarch64-apple-darwin/xcframework/build/zlib-sys-abc/out"
            )]
        );
    }

    /// `framework=`, `dylib=` and bare (system) libs are NOT archives to fold.
    #[test]
    fn ignores_non_static_link_kinds() {
        let json = r#"{"reason":"build-script-executed","package_id":"p#1","linked_libs":["framework=CoreFoundation","dylib=pthread","m"],"linked_paths":["native=/w/out"],"cfgs":[],"env":[],"out_dir":"/w/out"}"#;
        let p = parse_artifacts(json).unwrap();
        assert!(p.native.is_empty());
    }

    /// A `-sys` crate that emits the same `static=` lib twice (or two search
    /// paths) must yield exactly ONE request — the §2.4 duplicate-fold hazard.
    #[test]
    fn dedups_repeated_static_libs_per_package() {
        let json = r#"{"reason":"build-script-executed","package_id":"p#1","linked_libs":["static=onig","static=onig"],"linked_paths":["native=/w/a","native=/w/b"],"cfgs":[],"env":[],"out_dir":"/w/a"}"#;
        let p = parse_artifacts(json).unwrap();
        assert_eq!(p.native.len(), 1);
        assert_eq!(p.native[0].lib_name, "onig");
        assert_eq!(
            p.native[0].search_paths,
            vec![PathBuf::from("/w/a"), PathBuf::from("/w/b")]
        );
    }

    /// Non-JSON lines (cargo prints nothing else on stdout under
    /// --message-format=json, but be robust) and unrelated reasons are skipped.
    #[test]
    fn skips_unparseable_and_unrelated_lines() {
        let json = "not json\n{\"reason\":\"build-finished\",\"success\":true}\n";
        let p = parse_artifacts(json).unwrap();
        assert!(p.staticlibs.is_empty());
        assert!(p.native.is_empty());
    }

    /// A NESTED OUT_DIR (cmake-style `out/lib/libfoo.a`) resolves, because the
    /// search path cargo reports is the nested dir itself — the §2.4
    /// "non-recursive glob misses it" hazard, gone by construction.
    #[test]
    fn resolves_nested_out_dir_search_path() {
        let req = NativeArchiveRequest {
            package_id: "p#1".to_string(),
            lib_name: "onig".to_string(),
            search_paths: vec![PathBuf::from("/w/out/lib")],
        };
        let exists = |p: &Path| p == Path::new("/w/out/lib/libonig.a");
        assert_eq!(
            req.resolve_with(&exists).unwrap(),
            PathBuf::from("/w/out/lib/libonig.a")
        );
    }

    /// The first search path that HAS the archive wins; earlier misses are
    /// skipped rather than treated as failures.
    #[test]
    fn resolve_picks_first_existing_search_path() {
        let req = NativeArchiveRequest {
            package_id: "p#1".to_string(),
            lib_name: "z".to_string(),
            search_paths: vec![PathBuf::from("/w/a"), PathBuf::from("/w/b")],
        };
        let exists = |p: &Path| p == Path::new("/w/b/libz.a");
        assert_eq!(req.resolve_with(&exists).unwrap(), PathBuf::from("/w/b/libz.a"));
    }

    /// An unlocatable declared archive is a HARD ERROR — never a silent skip.
    /// Silently dropping it is exactly how §2.4's undefined symbols appear.
    #[test]
    fn resolve_errors_when_archive_is_missing() {
        let req = NativeArchiveRequest {
            package_id: "path+file:///w/crates-sys/zlib-sys#0.1.0".to_string(),
            lib_name: "z".to_string(),
            search_paths: vec![PathBuf::from("/w/a")],
        };
        let err = req.resolve_with(&|_| false).unwrap_err();
        assert!(err.contains("libz.a"), "error must name the archive: {err}");
        assert!(err.contains("zlib-sys"), "error must name the package: {err}");
    }
}
```

- [ ] **Step 3.4: Run the tests to verify they fail**

Run: `cargo test -p xtask archives:: 2>&1 | tail -20`
Expected: FAIL to compile — `cannot find function 'parse_artifacts' in this scope` and `cannot find struct 'NativeArchiveRequest' in this scope`.

- [ ] **Step 3.5: Write the implementation**

Prepend to `xtask/src/archives.rs` (above the `#[cfg(test)] mod tests` block):

```rust
//! Deterministic static-archive discovery for the xcframework combine step.
//!
//! PORTS: `ghostty/src/build/GhosttyLib.zig:52-57` (the enumerated
//! `deps.add(lib)` + `getEmittedBin()` list handed to the combine step) and
//! `ghostty/src/build/LibtoolStep.zig:54-78` (`normalizeArchive`).
//!
//! Why this module exists (INTEGRATION_PARITY §2.4/§2.5): xtask used to find
//! the `-sys` crates' archives with a one-level `read_dir` over
//! `target/<t>/xcframework/build/*/out/*.a`. That is a function of leftover
//! filesystem state, not of the dependency graph, and it fails two ways:
//!
//!   1. a crate whose archive lands in a NESTED OUT_DIR subdirectory (the
//!      cmake-style `out/lib/libfoo.a`) is silently missed -> undefined
//!      symbols at app-link;
//!   2. several hash-suffixed build dirs for the same crate leave more than one
//!      matching `.a`, so libtool folds stale/duplicate objects -> duplicate or
//!      wrong-version symbols.
//!
//! Cargo already knows the exact answer. Under `--message-format=json` every
//! build script reports the `cargo:rustc-link-lib` / `cargo:rustc-link-search`
//! directives it emitted, and the staticlib target reports its own output
//! filenames. This module turns that stream into an explicit set — the direct
//! analogue of the oracle's enumerated dep list — and treats an archive it
//! cannot locate as a HARD ERROR rather than a silent omission.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One `cargo:rustc-link-lib=static=<name>` a build script emitted, together
/// with every `cargo:rustc-link-search` path that same script emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeArchiveRequest {
    /// Cargo's package id, used only for error messages.
    pub package_id: String,
    /// The library name as declared (`z` for `static=z`), i.e. without the
    /// `lib` prefix and `.a` suffix.
    pub lib_name: String,
    /// The search paths that script emitted, in emission order.
    pub search_paths: Vec<PathBuf>,
}

impl NativeArchiveRequest {
    /// Resolve to a concrete `lib<name>.a`, using `exists` to probe the
    /// filesystem (injected so the resolution rules are unit-testable).
    ///
    /// The first search path that actually holds the archive wins. Failing to
    /// find it is an error, never a skip.
    pub fn resolve_with(&self, exists: &dyn Fn(&Path) -> bool) -> Result<PathBuf, String> {
        let file = format!("lib{}.a", self.lib_name);
        for dir in &self.search_paths {
            let candidate = dir.join(&file);
            if exists(&candidate) {
                return Ok(candidate);
            }
        }
        Err(format!(
            "package {} declared `static={}` but {file} was not found in any of its \
             link-search paths ({:?}). The combined archive would be missing its symbols.",
            self.package_id, self.lib_name, self.search_paths
        ))
    }

    /// [`Self::resolve_with`] against the real filesystem.
    pub fn resolve(&self) -> Result<PathBuf, String> {
        self.resolve_with(&|p: &Path| p.exists())
    }
}

/// Everything the combine step needs out of one cargo JSON message stream.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ParsedArtifacts {
    /// `.a` outputs of `staticlib` targets (rustty's own Rust staticlib).
    pub staticlibs: Vec<PathBuf>,
    /// Static archives the `-sys` build scripts declared, de-duplicated per
    /// (package, lib name) in first-seen order.
    pub native: Vec<NativeArchiveRequest>,
}

/// Parse a newline-delimited `cargo build --message-format=json` stream.
///
/// Unparseable lines and messages of other `reason`s are skipped: cargo mixes
/// `compiler-message`, `build-finished`, etc. into the same stream, and none of
/// them describe archives.
pub fn parse_artifacts(json: &str) -> Result<ParsedArtifacts, String> {
    let mut out = ParsedArtifacts::default();
    // (package_id, lib_name) pairs already recorded, so a script that emits the
    // same `static=` directive twice contributes one entry.
    let mut seen: Vec<(String, String)> = Vec::new();

    for line in json.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match msg.get("reason").and_then(|r| r.as_str()) {
            Some("compiler-artifact") => {
                let is_staticlib = msg
                    .get("target")
                    .and_then(|t| t.get("kind"))
                    .and_then(|k| k.as_array())
                    .is_some_and(|kinds| kinds.iter().any(|k| k.as_str() == Some("staticlib")));
                if !is_staticlib {
                    continue;
                }
                let files = msg.get("filenames").and_then(|f| f.as_array());
                for f in files.into_iter().flatten() {
                    let Some(s) = f.as_str() else { continue };
                    if s.ends_with(".a") {
                        out.staticlibs.push(PathBuf::from(s));
                    }
                }
            }
            Some("build-script-executed") => {
                let package_id = msg
                    .get("package_id")
                    .and_then(|p| p.as_str())
                    .unwrap_or("<unknown package>")
                    .to_string();

                // `linked_paths` are raw `cargo:rustc-link-search` values:
                // either `native=/dir` (or another kind prefix) or a bare dir.
                let search_paths: Vec<PathBuf> = msg
                    .get("linked_paths")
                    .and_then(|p| p.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| PathBuf::from(strip_kind(s)))
                            .collect()
                    })
                    .unwrap_or_default();

                let libs = msg.get("linked_libs").and_then(|l| l.as_array());
                for lib in libs.into_iter().flatten() {
                    let Some(spec) = lib.as_str() else { continue };
                    // Only `static=` entries name an archive we must fold.
                    // `framework=`, `dylib=` and bare names are system links
                    // the app resolves normally.
                    let Some(name) = spec.strip_prefix("static=") else {
                        continue;
                    };
                    // Cargo allows `static:+bundle=name` style modifiers; take
                    // the part after the last `=`.
                    let name = name.rsplit('=').next().unwrap_or(name);
                    let key = (package_id.clone(), name.to_string());
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.push(key);
                    out.native.push(NativeArchiveRequest {
                        package_id: package_id.clone(),
                        lib_name: name.to_string(),
                        search_paths: search_paths.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Strip a `kind=` prefix from a raw cargo link directive value
/// (`native=/dir` -> `/dir`; a bare `/dir` is returned unchanged).
fn strip_kind(value: &str) -> &str {
    match value.split_once('=') {
        Some((_, rest)) => rest,
        None => value,
    }
}

/// Run `cargo build … --message-format=json` and return its stdout.
///
/// `args` are the cargo arguments AFTER `build` (e.g. `["-p", "rustty-apprt-c",
/// "--profile", "xcframework"]`); `--message-format=json` is appended here so
/// no caller can forget it. `envs` are extra environment variables to set
/// (`MACOSX_DEPLOYMENT_TARGET`). stderr is inherited so the human still sees
/// cargo's progress and warnings.
pub fn cargo_build_json(
    root: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<String, String> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build").args(args).arg("--message-format=json");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd
        .current_dir(root)
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("spawn cargo build: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo build exited with {}", out.status));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("cargo JSON output was not UTF-8: {e}"))
}
```

- [ ] **Step 3.6: Run the tests to verify they pass**

Run: `cargo test -p xtask archives:: 2>&1 | tail -20`
Expected: PASS — 7 tests (`parses_staticlib_and_one_native_archive`, `ignores_non_static_link_kinds`, `dedups_repeated_static_libs_per_package`, `skips_unparseable_and_unrelated_lines`, `resolves_nested_out_dir_search_path`, `resolve_picks_first_existing_search_path`, `resolve_errors_when_archive_is_missing`).

- [ ] **Step 3.7: Commit**

```bash
cd /Users/gael/projects/rustty
git add xtask/Cargo.toml xtask/src/archives.rs xtask/src/main.rs Cargo.lock
git commit -m "xtask: derive the xcframework archive set from cargo --message-format=json (WS9 T3, INTEGRATION_PARITY 2.4)"
```

---

## Task 4: `xtask::archives` — `ranlib` normalization before `libtool` (§2.5)

**Files:**

- Modify: `xtask/src/archives.rs` (add `normalize_archive` + its test)

**Interfaces:**

- Consumes: nothing (self-contained shell-out).
- Produces (Task 5 consumes): `pub fn normalize_archive(src: &Path, dst: &Path) -> Result<(), String>` — copies `src` to `dst` and runs `/usr/bin/ranlib` on `dst`; `src` is left untouched.

- [ ] **Step 4.1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `xtask/src/archives.rs`:

```rust
    /// `normalize_archive` must produce a copy WITH a regenerated symbol table
    /// and leave the source alone. Built from a real object file so the
    /// assertion exercises the real archive format (macOS only — the fix is a
    /// Darwin `libtool` interoperability workaround).
    #[test]
    #[cfg(target_os = "macos")]
    fn normalize_archive_regenerates_the_symbol_table() {
        use std::process::Command;

        let tmp = std::env::temp_dir().join(format!("ws9-ranlib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let c = tmp.join("p.c");
        std::fs::write(&c, b"int ws9_probe(void){return 42;}\n").unwrap();
        let o = tmp.join("p.o");
        assert!(Command::new("clang")
            .args(["-c", "-o"])
            .arg(&o)
            .arg(&c)
            .status()
            .unwrap()
            .success());

        // `ar rcS` deliberately omits the symbol table (`S` = no index) — the
        // "unexpected layout" shape LibtoolStep.zig warns about.
        let src = tmp.join("libws9probe.a");
        assert!(Command::new("/usr/bin/ar")
            .arg("rcS")
            .arg(&src)
            .arg(&o)
            .status()
            .unwrap()
            .success());

        let toc = |p: &Path| -> String {
            let out = Command::new("/usr/bin/ar").arg("-t").arg(p).output().unwrap();
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        assert!(
            !toc(&src).contains("__.SYMDEF"),
            "precondition: the source archive must have no symbol table"
        );

        let dst = tmp.join("0-libRusttyKit.a");
        normalize_archive(&src, &dst).unwrap();

        assert!(
            toc(&dst).contains("__.SYMDEF"),
            "normalized archive must carry a regenerated symbol table"
        );
        assert!(
            !toc(&src).contains("__.SYMDEF"),
            "the source archive must not be modified in place"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
```

- [ ] **Step 4.2: Run the test to verify it fails**

Run: `cargo test -p xtask archives::tests::normalize_archive 2>&1 | tail -20`
Expected: FAIL to compile — `cannot find function 'normalize_archive' in this scope`.

- [ ] **Step 4.3: Write the implementation**

Add to `xtask/src/archives.rs`, after the `cargo_build_json` function:

```rust
/// Copy `src` to `dst` and run `/usr/bin/ranlib` on the copy.
///
/// PORTS `ghostty/src/build/LibtoolStep.zig:54-78` `normalizeArchive`, verbatim
/// in intent (the oracle runs `/bin/cp "$1" "$2" && /usr/bin/ranlib "$2"` on
/// EVERY source archive before handing it to `libtool`).
///
/// Why: newer Xcode `libtool` can silently DROP 64-bit archive members when the
/// input archive layout is not what it expects — producing a combined archive
/// that is missing symbols, nondeterministically, with no error. `ranlib`
/// rewrites the archive with a regenerated symbol table, normalizing the
/// layout. rustty is more exposed than the oracle here, because every archive it
/// folds comes from a `cc`/`ar` build script — exactly the layout the oracle's
/// comment warns about (INTEGRATION_PARITY §2.5).
///
/// The copy matters: the sources are cargo's own build outputs and must not be
/// rewritten in place, or cargo's freshness tracking sees them change.
pub fn normalize_archive(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let _ = std::fs::remove_file(dst);
    std::fs::copy(src, dst)
        .map_err(|e| format!("copy {} -> {}: {e}", src.display(), dst.display()))?;
    let status = Command::new("/usr/bin/ranlib")
        .arg(dst)
        .status()
        .map_err(|e| format!("spawn /usr/bin/ranlib: {e}"))?;
    if !status.success() {
        return Err(format!("ranlib {} exited with {status}", dst.display()));
    }
    Ok(())
}
```

- [ ] **Step 4.4: Run the test to verify it passes**

Run: `cargo test -p xtask archives:: 2>&1 | tail -20`
Expected: PASS — 8 tests total (the 7 from Task 3 plus `normalize_archive_regenerates_the_symbol_table`).

- [ ] **Step 4.5: Commit**

```bash
cd /Users/gael/projects/rustty
git add xtask/src/archives.rs
git commit -m "xtask: port LibtoolStep normalizeArchive (cp + ranlib per source archive) (WS9 T4, INTEGRATION_PARITY 2.5)"
```

---

## Task 5: Wire the new combine pipeline + drop `-lintl` from the app link

**Files:**

- Modify: `xtask/src/main.rs:320-388` (the cargo build + combine block inside `xcframework()`)
- Modify: `macos/build-rustty.sh:19-31` (comment block + `OTHER_LDFLAGS`)

**Interfaces:**

- Consumes: `archives::cargo_build_json`, `archives::parse_artifacts`, `archives::NativeArchiveRequest::resolve`, `archives::normalize_archive` (signatures in Tasks 3 and 4); the `rustty_intl` archive from Task 1.
- Produces: `target/aarch64-apple-darwin/xcframework/libRusttyKit.a` defines `_dgettext`, `_deflate` and has NO undefined `_dgettext`/`_deflate`; `dist/RusttyKit.xcframework`; `Rustty.app` links no `libintl` dylib.

- [ ] **Step 5.1: Replace the cargo-build + combine block in `xcframework()`**

In `xtask/src/main.rs`, replace lines 320-388 (from `let status = Command::new("cargo")` through the closing `}` of the `if !lib.exists() { … }` check) with:

```rust
    let json = archives::cargo_build_json(
        &root,
        &[
            "-p",
            "rustty-apprt-c",
            "--profile",
            "xcframework",
            // SIMD on for the shipped product library, matching upstream
            // Ghostty's default-on lib and the CMake wrapper's RUSTTY_VT_SIMD
            // default. (rustty's plain-`cargo build` default stays SIMD-off.)
            "--features",
            "simd",
            "--target",
            target,
        ],
        &[("MACOSX_DEPLOYMENT_TARGET", MACOS_DEPLOYMENT_TARGET)],
    )?;

    // 1b. Combine the Rust staticlib with its vendored C archives into ONE
    // self-contained lib (ghostty's CombineArchivesStep). A Rust `staticlib`
    // records its C link deps as metadata but does NOT bundle the -sys crates'
    // static archives (zlib/gettext/harfbuzz/freetype/oniguruma/…), so the final
    // xcodebuild link would report undefined `_deflate`/`_dgettext`/… . libtool
    // folds rustty's OWN vendored C archives (R3 — not Homebrew/system) into the
    // shipped lib so the Swift app links against a single self-contained archive.
    //
    // WS9: the set is DERIVED FROM CARGO (the `--message-format=json` stream
    // above), not from a filesystem glob — the port of GhosttyLib.zig's
    // enumerated `deps.add(lib)` list. A declared archive that cannot be located
    // is a hard error, so a nested OUT_DIR can no longer be silently missed and
    // stale hash-suffixed build dirs can no longer be folded in
    // (INTEGRATION_PARITY §2.4).
    let parsed = archives::parse_artifacts(&json)?;
    let rust_lib = match parsed.staticlibs.as_slice() {
        [one] => one.clone(),
        [] => return Err("cargo reported no staticlib artifact for rustty-apprt-c".to_string()),
        many => {
            return Err(format!(
                "cargo reported {} staticlib artifacts; expected exactly one: {many:?}",
                many.len()
            ))
        }
    };
    let mut archives_list: Vec<PathBuf> = vec![rust_lib.clone()];
    for req in &parsed.native {
        archives_list.push(req.resolve()?);
    }
    eprintln!(
        "xcframework: folding {} archives ({} Rust staticlib + {} vendored C)",
        archives_list.len(),
        1,
        parsed.native.len()
    );

    // WS9: normalize EVERY source archive with ranlib before libtool sees it
    // (ghostty LibtoolStep.zig `normalizeArchive`) — newer Xcode libtool can
    // silently drop 64-bit members from cc/ar-produced archives
    // (INTEGRATION_PARITY §2.5).
    let norm_dir = root.join(format!("target/{target}/xcframework/normalized"));
    let _ = std::fs::remove_dir_all(&norm_dir);
    let mut normalized: Vec<PathBuf> = Vec::with_capacity(archives_list.len());
    for (i, a) in archives_list.iter().enumerate() {
        let dst = norm_dir.join(format!("{i}-libRusttyKit.a"));
        archives::normalize_archive(a, &dst)?;
        normalized.push(dst);
    }

    let lib = root.join(format!("target/{target}/xcframework/libRusttyKit.a"));
    let _ = std::fs::remove_file(&lib);
    let mut lt = Command::new("libtool");
    lt.arg("-static").arg("-o").arg(&lib);
    for a in &normalized {
        lt.arg(a);
    }
    let status = lt
        .current_dir(&root)
        .status()
        .map_err(|e| format!("spawn libtool: {e}"))?;
    if !status.success() {
        return Err(format!(
            "libtool combine exited with {status} ({} archives)",
            normalized.len()
        ));
    }
    if !lib.exists() {
        return Err(format!("combined lib not found at {}", lib.display()));
    }
```

- [ ] **Step 5.2: Build the xcframework and verify the archive set is explicit**

Run:

```bash
cd /Users/gael/projects/rustty
rm -rf target/aarch64-apple-darwin/xcframework
cargo run -q -p xtask -- xcframework 2>&1 | tail -20
```

Expected: a line `xcframework: folding N archives (1 Rust staticlib + M vendored C)` with `M >= 10` (zlib, gettext, harfbuzz, freetype, oniguruma, libpng, libxml2, fontconfig, sentry, simdutf, highway, wuffs, …), then the `xcodebuild -create-xcframework` output, exit 0. **No** "was not found in any of its link-search paths" error.

- [ ] **Step 5.3: Assert the combined archive is self-contained for gettext and zlib**

Run:

```bash
cd /Users/gael/projects/rustty
LIB=target/aarch64-apple-darwin/xcframework/libRusttyKit.a
echo "--- defined ---"
nm -g "$LIB" 2>/dev/null | grep -E " T _(dgettext|bindtextdomain|textdomain|deflate)$" | sort -u
echo "--- undefined (must be empty) ---"
nm -u "$LIB" 2>/dev/null | grep -E "^_(dgettext|bindtextdomain|textdomain|deflate)$" | sort -u
echo "--- symbol table present ---"
ar -t "$LIB" | head -1
```

Expected:

- defined: four lines — `T _bindtextdomain`, `T _deflate`, `T _dgettext`, `T _textdomain`.
- undefined: **empty**.
- the `ar -t` first entry is `__.SYMDEF SORTED`.

(`nm -u` on a static archive lists per-member undefineds, including ones satisfied by another member of the same archive; the grep above is scoped to the four symbols whose definitions we just asserted are present, so any hit means a member genuinely could not be satisfied from within the archive.)

- [ ] **Step 5.4: Drop `-lintl` from `macos/build-rustty.sh`**

Replace lines 20-26 (the comment block above `cd macos`) with:

```bash
# The combined archive is fully self-contained — no system or Homebrew link
# flags for zlib or gettext:
#   * zlib: rustty's vendored libz.a builds FULL zlib (inflate + deflate; xtask
#     P3 vendored deflate.c/trees.c/compress.c), so sentry's `deflate` resolves
#     from the app's own archive — no `-lz`.
#   * gettext: rustty's vendored gettext-sys builds the FULL macOS libintl (WS9
#     vendored the reference's whole pkg/libintl `srcs` set), so
#     `dgettext`/`bindtextdomain`/`textdomain` resolve from the app's own
#     archive — no `-L/opt/homebrew/opt/gettext/lib -lintl`.
# `-lstdc++` stays: it is a toolchain runtime (the C++ -sys crates), not a
# third-party library rustty could vendor.
# Guard: `.port/tools/gates/no_homebrew_gettext_gate.sh` proves this by building
# with Homebrew's libintl hidden.
```

and replace line 31 with:

```bash
  OTHER_LDFLAGS="-lstdc++" \
```

- [ ] **Step 5.5: Build the app and verify it does not link Homebrew libintl**

Run:

```bash
cd /Users/gael/projects/rustty
bash macos/build-rustty.sh 2>&1 | tail -20
```

Expected: `** BUILD SUCCEEDED **`.

Then:

```bash
cd /Users/gael/projects/rustty
BUILT=$(xcodebuild -project macos/Rustty.xcodeproj -scheme Rustty -configuration Debug \
  -showBuildSettings 2>/dev/null | awk -F' = ' '/ BUILT_PRODUCTS_DIR /{print $2; exit}')
BIN="$BUILT/Rustty.app/Contents/MacOS/rustty"
echo "binary: $BIN"
otool -L "$BIN" | grep -i intl && echo "FAIL: links libintl" || echo "OK: no libintl dylib"
otool -L "$BIN" | grep -i homebrew && echo "FAIL: links Homebrew" || echo "OK: no Homebrew dylib"
nm -u "$BIN" 2>/dev/null | grep -E "_(dgettext|bindtextdomain|textdomain)$" && echo "FAIL: undefined gettext" || echo "OK: gettext resolved"
```

Expected: `OK: no libintl dylib`, `OK: no Homebrew dylib`, `OK: gettext resolved`.

- [ ] **Step 5.6: Commit**

```bash
cd /Users/gael/projects/rustty
git add xtask/src/main.rs macos/build-rustty.sh
git commit -m "xtask+macos: explicit archive set + ranlib normalization; drop the Homebrew -lintl app link (WS9 T5)"
```

---

## Task 6: The hermetic no-Homebrew-gettext acceptance gate

**Files:**

- Create: `.port/tools/gates/no_homebrew_gettext_gate.sh`

**Interfaces:**

- Consumes: `macos/build-rustty.sh` (Task 5), the vendored libintl (Task 1), the Homebrew-free build scripts (Task 2).
- Produces: an executable gate script; a green run of it is the WS9 acceptance criterion.

**Design note (read before implementing):** the gate must disable Homebrew's libintl for the _linker_ without breaking Homebrew's `msgfmt`, which `xtask resources` legitimately needs at build time (the oracle needs it too — `GhosttyI18n.zig`). Renaming the whole `gettext` prefix would break `msgfmt` and prove nothing about linking. So the gate renames exactly the two linker-facing files inside the Cellar — `lib/libintl.dylib` (the unversioned dev symlink) and `lib/libintl.a` — which simultaneously dangles `/opt/homebrew/opt/gettext/lib/libintl.{dylib,a}` and `/opt/homebrew/lib/libintl.{dylib,a}` (both are symlinks into that dir). `ld` resolves `-lintl` only via `libintl.dylib`/`libintl.tbd`/`libintl.a`, never `libintl.8.dylib`, so after the rename `-lintl` is unresolvable host-wide while `msgfmt` (which loads `libintl.8.dylib` by absolute path) keeps working. Both renames are reversed by an `EXIT` trap, so an interrupted run still restores the host.

- [ ] **Step 6.1: Write the gate script**

Create `.port/tools/gates/no_homebrew_gettext_gate.sh`:

```bash
#!/bin/bash
# WS9 acceptance gate — prove the macOS app is SELF-CONTAINED for gettext.
#
# INTEGRATION_PARITY §2.3 required that the shipped archive stop depending on
# Homebrew's libintl. This gate proves it the only way that cannot be faked:
# it makes Homebrew's libintl UNLINKABLE, then does a full clean build of
# Rustty.app and asserts the gettext symbols still resolve from rustty's own
# vendored archive.
#
# How the "hide" works (and why it is surgical):
#   /opt/homebrew/opt/gettext        -> ../Cellar/gettext/<v>
#   /opt/homebrew/lib/libintl.dylib  -> ../Cellar/gettext/<v>/lib/libintl.dylib
#   /opt/homebrew/lib/libintl.a      -> ../Cellar/gettext/<v>/lib/libintl.a
# Renaming the two Cellar files dangles every one of those paths at once, so
# `-lintl` cannot resolve anywhere on the host. `libintl.8.dylib` is left in
# place because Homebrew's `msgfmt` loads it by ABSOLUTE path — and `msgfmt` is
# a legitimate BUILD-TIME TOOL dependency (`xtask resources` compiles po/*.po;
# the oracle's GhosttyI18n.zig needs it too). WS9's claim is about what the
# shipped binary LINKS, not about which tools build it.
#
# The renames are reverted by an EXIT trap, so ^C or a mid-build failure still
# leaves the host exactly as it was.
#
# Usage:  bash .port/tools/gates/no_homebrew_gettext_gate.sh
# Exit 0 = self-contained. Any non-zero = the gate failed; read the message.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$ROOT"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }
ok()   { echo "  ok: $*"; }

# ---------------------------------------------------------------- 0. static
echo "== 0/5 static checks: no source references Homebrew gettext for LINKING =="
# Only EFFECTIVE flags count, so strip `#` / `//` comment lines first (both files
# retain explanatory comments that mention `-lintl` by name on purpose).
strip_comments() { sed -e 's/[[:space:]]*#.*$//' -e 's|[[:space:]]*//.*$||' "$1"; }
for f in macos/build-rustty.sh crates/*/build.rs crates-sys/*/build.rs; do
  if strip_comments "$f" | grep -q -- "-lintl\|rustc-link-lib=dylib=intl\|opt/gettext/lib"; then
    fail "$f still passes a Homebrew gettext link flag"
  fi
done
ok "no -lintl link flag in any build script or in build-rustty.sh"

# --------------------------------------------------------------- 1. locate
echo "== 1/5 locating Homebrew's libintl =="
CELLAR_LIB=""
for prefix in /opt/homebrew /usr/local; do
  cand="$prefix/opt/gettext/lib"
  if [ -e "$cand/libintl.dylib" ] || [ -e "$cand/libintl.a" ]; then
    # Resolve through the `opt` symlink to the real Cellar directory so the
    # rename touches the file, not a symlink chain.
    CELLAR_LIB="$(cd "$cand" && pwd -P)"
    break
  fi
done
if [ -z "$CELLAR_LIB" ]; then
  echo "  note: no Homebrew gettext found on this host — the build is already"
  echo "        running in the hermetic condition this gate creates."
else
  ok "found Homebrew libintl at $CELLAR_LIB"
fi

# ------------------------------------------------------------------ 2. hide
HIDDEN=()
restore() {
  for f in "${HIDDEN[@]:-}"; do
    [ -n "$f" ] || continue
    if [ -e "$f.ws9-hidden" ] || [ -L "$f.ws9-hidden" ]; then
      mv "$f.ws9-hidden" "$f" && echo "  restored $f"
    fi
  done
}
trap restore EXIT

echo "== 2/5 hiding Homebrew's linker-facing libintl =="
if [ -n "$CELLAR_LIB" ]; then
  for f in "$CELLAR_LIB/libintl.dylib" "$CELLAR_LIB/libintl.a"; do
    if [ -e "$f" ] || [ -L "$f" ]; then
      mv "$f" "$f.ws9-hidden" || fail "could not rename $f (is $CELLAR_LIB writable?)"
      HIDDEN+=("$f")
      echo "  hid $f"
    fi
  done
  # Prove the hide actually took: a trivial -lintl link must now FAIL.
  probe="$(mktemp -d)"
  printf 'int main(void){return 0;}\n' > "$probe/p.c"
  if clang -o "$probe/p" "$probe/p.c" -L/opt/homebrew/lib -L"$CELLAR_LIB" -lintl 2>/dev/null; then
    rm -rf "$probe"
    fail "-lintl STILL links after hiding; the gate would prove nothing"
  fi
  rm -rf "$probe"
  ok "-lintl is now unresolvable host-wide"
  # msgfmt must survive — it is a build-time tool, not a link dependency.
  command -v msgfmt >/dev/null 2>&1 || PATH="/opt/homebrew/opt/gettext/bin:$PATH"
  msgfmt --version >/dev/null 2>&1 || fail "msgfmt broke; the hide was too broad"
  ok "msgfmt still runs (build-time tool preserved)"
fi

# ----------------------------------------------------------------- 3. build
echo "== 3/5 clean build of Rustty.app with libintl hidden =="
rm -rf target/aarch64-apple-darwin/xcframework
if ! bash macos/build-rustty.sh > /tmp/ws9-gate-build.log 2>&1; then
  tail -40 /tmp/ws9-gate-build.log >&2
  fail "build-rustty.sh failed without Homebrew gettext (see /tmp/ws9-gate-build.log)"
fi
grep -q "BUILD SUCCEEDED" /tmp/ws9-gate-build.log \
  || fail "xcodebuild did not report BUILD SUCCEEDED (see /tmp/ws9-gate-build.log)"
ok "build-rustty.sh succeeded with no Homebrew libintl available"

# --------------------------------------------------------------- 4. archive
echo "== 4/5 symbol assertions on the combined archive =="
LIB="target/aarch64-apple-darwin/xcframework/libRusttyKit.a"
[ -f "$LIB" ] || fail "combined archive missing at $LIB"
for sym in _dgettext _bindtextdomain _textdomain _deflate; do
  nm -g "$LIB" 2>/dev/null | grep -q " T $sym\$" \
    || fail "$LIB does not DEFINE $sym"
  ok "$LIB defines $sym"
done
for sym in _dgettext _bindtextdomain _textdomain _deflate; do
  if nm -u "$LIB" 2>/dev/null | grep -qx "$sym"; then
    fail "$LIB has UNDEFINED $sym"
  fi
done
ok "no undefined _dgettext/_bindtextdomain/_textdomain/_deflate"
ar -t "$LIB" | head -1 | grep -q "__.SYMDEF" \
  || fail "$LIB has no symbol table (ranlib normalization did not run)"
ok "combined archive carries a symbol table"

# ------------------------------------------------------------------ 5. app
echo "== 5/5 symbol assertions on the linked app binary =="
BUILT="$(xcodebuild -project macos/Rustty.xcodeproj -scheme Rustty -configuration Debug \
  -showBuildSettings 2>/dev/null | awk -F' = ' '/ BUILT_PRODUCTS_DIR /{print $2; exit}')"
BIN="$BUILT/Rustty.app/Contents/MacOS/rustty"
[ -x "$BIN" ] || fail "app binary not found at $BIN"
otool -L "$BIN" | grep -qi "intl" && fail "$BIN links a libintl dylib"
ok "app binary links no libintl dylib"
otool -L "$BIN" | grep -qi "homebrew\|/usr/local/opt" && fail "$BIN links a Homebrew dylib"
ok "app binary links no Homebrew dylib"
for sym in _dgettext _bindtextdomain _textdomain; do
  nm -u "$BIN" 2>/dev/null | grep -qx "$sym" && fail "$BIN has undefined $sym"
done
ok "app binary has no undefined gettext symbols"

echo
echo "GATE PASS: Rustty.app builds and links with NO Homebrew gettext available."
```

- [ ] **Step 6.2: Make it executable and run it**

Run:

```bash
cd /Users/gael/projects/rustty
chmod +x .port/tools/gates/no_homebrew_gettext_gate.sh
bash .port/tools/gates/no_homebrew_gettext_gate.sh
echo "exit=$?"
```

Expected: the five sections each print `ok:` lines, ending with `GATE PASS: Rustty.app builds and links with NO Homebrew gettext available.` and `exit=0`. Every `restored …` line must appear before the process exits.

- [ ] **Step 6.3: Verify the host was restored**

Run:

```bash
ls -l /opt/homebrew/opt/gettext/lib/libintl.dylib /opt/homebrew/opt/gettext/lib/libintl.a
ls /opt/homebrew/opt/gettext/lib/*.ws9-hidden 2>/dev/null && echo "FAIL: leftovers" || echo "OK: no leftovers"
```

Expected: both files listed normally; `OK: no leftovers`.

- [ ] **Step 6.4: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/tools/gates/no_homebrew_gettext_gate.sh
git commit -m "gates: hermetic no-Homebrew-gettext acceptance gate for the self-contained xcframework (WS9 T6)"
```

---

## Task 7: Deployment-target re-verification (already settled by WS3 — verify only)

**Files:**

- Modify: none expected. (`xtask/src/main.rs:301` `MACOS_DEPLOYMENT_TARGET = "13.0"` and the `.env(...)` at the call built in Task 5 are already correct.)

**Interfaces:**

- Consumes: the Task 5 xcframework build (which passes `MACOSX_DEPLOYMENT_TARGET` through `archives::cargo_build_json`'s `envs`).
- Produces: evidence that the WS9 pipeline rewrite did not regress the deployment-target pinning. Nothing later depends on it.

**Context (do NOT redo):** WS3 already pinned `MACOSX_DEPLOYMENT_TARGET=13.0` and verified it end to end (`macos/MILESTONE.md`, "Deployment target: ✅ DONE (WS3, 2026-07-23)"). WS3 also established that ~383 Rust-**sysroot** objects (compiler_builtins/alloc/adler2/…) carry the toolchain's own `minos 11.0` stamp; 11.0 is OLDER than the 13.0 deployment target, so it can never trigger the "built for newer macOS version" warning. Task 5 moved the cargo invocation into `archives::cargo_build_json`, so the only open question is whether the env var still reaches rustc and the `cc` build scripts.

- [ ] **Step 7.1: Verify the env var still reaches every object our build compiles**

Run:

```bash
cd /Users/gael/projects/rustty
LIB=target/aarch64-apple-darwin/xcframework/libRusttyKit.a
otool -l "$LIB" 2>/dev/null | grep -A3 LC_BUILD_VERSION | grep "minos" | sort | uniq -c
```

Expected: two buckets only — a large count of `minos 13.0` (our crates plus every `-sys` build-script object) and a `minos 11.0` bucket of at most a few hundred (the pre-built Rust sysroot objects WS3 identified). **No bucket newer than 13.0.**

- [ ] **Step 7.2: Verify the app link log is warning-free**

Run:

```bash
cd /Users/gael/projects/rustty
grep -c "was built for newer macOS version" /tmp/ws9-gate-build.log
```

Expected: `0` (the gate's build log from Task 6 is the freshest full app build).

- [ ] **Step 7.3: Record the result — no code change if both checks pass**

If both checks pass, make NO code change; the deployment-target item stays "DONE (WS3)" and Task 8's ledger edit only notes WS9 re-verified it after the pipeline rewrite.

If Step 7.1 shows a bucket newer than 13.0, the env forwarding regressed: add `.env("CFLAGS", format!("-mmacosx-version-min={MACOS_DEPLOYMENT_TARGET}"))`-style forwarding to `archives::cargo_build_json`'s `envs` array at the `xcframework()` call site — i.e. extend the array to:

```rust
        &[
            ("MACOSX_DEPLOYMENT_TARGET", MACOS_DEPLOYMENT_TARGET),
            ("CFLAGS", "-mmacosx-version-min=13.0"),
            ("CXXFLAGS", "-mmacosx-version-min=13.0"),
        ],
```

then re-run Step 7.1 and commit with `xtask: forward the deployment target to cc builds explicitly (WS9 T7)`.

---

## Task 8: Both-platform gate, ledger strikes, merge

**Files:**

- Modify: `.port/phase-2/INTEGRATION_PARITY.md` (§2.3 lines 83-102, §2.4 lines 107-118, §2.5 lines 120-131; and the §5 "Fix List" entries for those three)
- Modify: `.port/phase-2/DEFERRALS.md` (the `## gettext-sys — self-contained libintl …` section beginning at line 4542)
- Modify: `macos/MILESTONE.md` (lines 46-49, the "Self-contained lib" follow-up; line 77, the "P3 polish" line)

**Interfaces:**

- Consumes: everything from Tasks 1-7.
- Produces: truthful ledgers and a merged `main`.

- [ ] **Step 8.1: Run the full macOS workspace gates**

Run:

```bash
cd /Users/gael/projects/rustty
cargo build --workspace 2>&1 | tail -5
cargo test -p gettext-sys -p rustty-os -p xtask 2>&1 | tail -20
bash .port/tools/gates/check_acyclic.sh && echo "ACYCLIC OK"
```

Expected: build exit 0; all three test suites `test result: ok`; `ACYCLIC OK`.

- [ ] **Step 8.2: Run the Linux gate on the VM**

Run:

```bash
cd /Users/gael/projects/rustty
git push -u origin ws9-packaging
ssh rvm 'cd ~/projects/rustty && git fetch --all -q && git checkout ws9-packaging && git pull -q && cargo build --workspace 2>&1 | tail -5'
```

Expected: exit 0, `Finished` line. (`gettext-sys`'s build script is a no-op on non-macOS — `build.rs` returns early when `CARGO_CFG_TARGET_OS != "macos"` — so the new vendor set is never compiled there, and `rustty-os/build.rs` lost only a macOS-gated block.)

- [ ] **Step 8.3: Re-run the acceptance gate on the final tree**

Run:

```bash
cd /Users/gael/projects/rustty
bash .port/tools/gates/no_homebrew_gettext_gate.sh && echo "ACCEPTANCE OK"
```

Expected: `GATE PASS: …` then `ACCEPTANCE OK`.

- [ ] **Step 8.4: Strike INTEGRATION_PARITY §2.3**

Replace the §2.3 heading (line 83) with:

```markdown
### 2.3 [RESOLVED 2026-07-28, WS9] Combined archive IS self-contained — full zlib (P3) + full gettext (WS9); `-lz`/`-lintl` both gone
```

and append this paragraph at the end of the §2.3 body (after the `**Fix sketch**` bullet):

```markdown
- **RESOLUTION**: the zlib half was already closed by the P3 pass (`crates-sys/
zlib-sys` vendors `deflate.c`/`trees.c`/`compress.c`; `-lz` dropped) — that
  part of the text above was stale when written. WS9 closed the gettext half:
  `crates-sys/gettext-sys` now vendors the reference's FULL `pkg/libintl/
build.zig` `srcs` set (30 TUs from gettext 0.24, incl. the pre-generated Bison
  `plural.c`) and emits `librustty_intl.a` defining `_dgettext`/
  `_bindtextdomain`/`_textdomain`. `-L/opt/homebrew/opt/gettext/lib -lintl` was
  removed from BOTH link sites — `macos/build-rustty.sh` `OTHER_LDFLAGS` (now
  just `-lstdc++`) and `crates/rustty-os/build.rs` (the WS4-added block; review
  finding **WS4-M1**, which also hardcoded the ARM-Homebrew prefix and missed
  Intel Homebrew — RESOLVED here). Proof:
  `.port/tools/gates/no_homebrew_gettext_gate.sh` renames Homebrew's
  linker-facing `libintl.{dylib,a}`, verifies `-lintl` no longer links, runs a
  clean `macos/build-rustty.sh`, and asserts `nm`/`otool` symbol conditions on
  both `libRusttyKit.a` and the app binary. Homebrew `msgfmt` remains a
  build-TIME tool dependency of `xtask resources` — the oracle needs it too
  (`GhosttyI18n.zig`) — and is out of scope here.
```

- [ ] **Step 8.5: Strike INTEGRATION_PARITY §2.4 and §2.5**

Replace the §2.4 heading (line 107) with:

```markdown
### 2.4 [RESOLVED 2026-07-28, WS9] Archive set is now an explicit cargo-derived dependency list, not a filesystem glob
```

and append to its body:

```markdown
- **RESOLUTION**: `xtask/src/archives.rs` (`parse_artifacts` +
  `NativeArchiveRequest::resolve`) derives the set from `cargo build
--message-format=json` — the staticlib target's own `filenames`, plus every
  `cargo:rustc-link-lib=static=<name>` a build script declared, resolved against
  that same script's `cargo:rustc-link-search` paths and de-duplicated per
  (package, lib). A nested OUT_DIR now resolves by construction (cargo reports
  the nested dir), stale hash-suffixed build dirs are never consulted, and an
  archive that cannot be located is a HARD ERROR instead of a silent omission.
  Unit-tested in `xtask/src/archives.rs`.
```

Replace the §2.5 heading (line 120) with:

```markdown
### 2.5 [RESOLVED 2026-07-28, WS9] ranlib per-source normalization runs before libtool
```

and append to its body:

```markdown
- **RESOLUTION**: `xtask::archives::normalize_archive` ports
  `LibtoolStep.zig`'s `normalizeArchive` — every source archive is copied to
  `target/<t>/xcframework/normalized/<i>-libRusttyKit.a` and `/usr/bin/ranlib`'d
  before `libtool -static` sees it. Unit-tested (`ar -t` shows `__.SYMDEF`
  appearing on the normalized copy and the source left unmodified) and asserted
  end-to-end by the acceptance gate.
```

- [ ] **Step 8.6: Update the §5 Fix List**

In `.port/phase-2/INTEGRATION_PARITY.md` §5, mark the three WS9 entries. Run this to find them:

```bash
cd /Users/gael/projects/rustty
sed -n '/^## 5\. Fix List/,$p' .port/phase-2/INTEGRATION_PARITY.md | grep -n "2\.3\|2\.4\|2\.5"
```

For each matched line, prefix its description with `✅ DONE (WS9, 2026-07-28) — ` leaving the rest of the line intact.

- [ ] **Step 8.7: Strike the DEFERRALS.md gettext entry**

In `.port/phase-2/DEFERRALS.md`, replace the section heading at line 4542 with:

```markdown
## ✅ RESOLVED 2026-07-28 (WS9) — gettext-sys self-contained libintl (`dgettext`/`bindtextdomain`/`textdomain`)
```

and replace the final `**Until then `-lintl` stays**` paragraph (the one beginning "`libGhosttyKit.a` will then resolve those symbols). **Until then `-lintl`") with:

```markdown
`libRusttyKit.a` will then resolve those symbols). **RESOLVED by WS9**: all 30
translation units (the reference's full `srcs`) are vendored under
`crates-sys/gettext-sys/vendor/intl/`, `build.rs` compiles them with the
reference's exact flags into `librustty_intl.a`, and BOTH `-lintl` link sites
are gone (`macos/build-rustty.sh` and `crates/rustty-os/build.rs`). `plural.c`
needed no Bison — it ships pre-generated in the gettext 0.24 tarball. Proven by
`.port/tools/gates/no_homebrew_gettext_gate.sh`.
```

- [ ] **Step 8.8: Strike the MILESTONE.md entries**

In `macos/MILESTONE.md`, replace lines 46-49 (the `- **Self-contained lib**: …` bullet) with:

```markdown
- **Self-contained lib**: ✅ DONE (zlib in the P3 pass, gettext in WS9 2026-07-28).
  The combined `libRusttyKit.a` carries FULL zlib (inflate + deflate, for sentry's
  `deflate`) and FULL libintl (`dgettext`/`bindtextdomain`/`textdomain`, from the
  30-TU vendor of gettext 0.24 mirroring `ghostty/pkg/libintl/build.zig`). Both
  `-lz` and `-L/opt/homebrew/opt/gettext/lib -lintl` are gone from
  `OTHER_LDFLAGS`; `crates/rustty-os/build.rs` no longer adds a Homebrew
  link-search either (review finding WS4-M1, resolved). WS9 also replaced xtask's
  filesystem-glob archive discovery with a `cargo --message-format=json`-derived
  set and added the `ranlib` normalization the oracle's LibtoolStep runs. Proof:
  `bash .port/tools/gates/no_homebrew_gettext_gate.sh` builds Rustty.app with
  Homebrew's libintl renamed away and asserts the `nm`/`otool` conditions.
```

and replace line 77 (`- **P3 polish:** autoreleasepool shim, self-contained zlib+gettext lib, R2 identity rebrand.`) with:

```markdown
- **P3 polish:** autoreleasepool shim (WS2); self-contained zlib+gettext lib ✅ DONE
  (zlib P3, gettext WS9); R2 identity rebrand ✅ DONE.
```

Also append to the "Deployment target" bullet (the `✅ DONE (WS3, 2026-07-23)` line) the sentence:

```markdown
Re-verified by WS9 after the xcframework pipeline rewrite (otool `minos`
buckets are 13.0 + the toolchain's own sysroot 11.0; zero "built for newer
macOS version" warnings in the app link log).
```

- [ ] **Step 8.9: Commit the ledger strikes**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/INTEGRATION_PARITY.md .port/phase-2/DEFERRALS.md macos/MILESTONE.md
git commit -m "ledgers: strike INTEGRATION_PARITY 2.3/2.4/2.5, the gettext deferral, and the MILESTONE self-contained-lib item (WS9 T8)"
```

- [ ] **Step 8.10: Merge to main and delete the branch**

Run:

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull
git merge --no-ff ws9-packaging -m "merge: WS9 self-contained xcframework + robust packaging"
cargo build --workspace 2>&1 | tail -3
git push
ssh rvm 'cd ~/projects/rustty && git checkout main && git pull -q && cargo build --workspace 2>&1 | tail -3'
git branch -d ws9-packaging && git push origin --delete ws9-packaging
```

Expected: merge commit created; both builds exit 0; branch deleted locally and remotely.

---

## Self-Review (performed at authoring time)

**1. Spec coverage.**

| Spec item                                                                                  | Task                                                                                                                                             |
| ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| (a) Vendor full libintl into `gettext-sys` so `out/` carries `_dgettext`                   | Task 1                                                                                                                                           |
| (a) Drop `-lintl` from `macos/build-rustty.sh`                                             | Task 5 (Step 5.4)                                                                                                                                |
| (a) Drop `-lintl` from `crates/rustty-os/build.rs` (WS4-M1, incl. the Intel-Homebrew miss) | Task 2                                                                                                                                           |
| (a) VERIFY the zlib deflate claim, scope accordingly                                       | "Pre-verified facts" bullet 1 (already fixed — no work), asserted anyway in Steps 5.3 and 6.4, corrected in the §2.3 ledger text (Step 8.4)      |
| (b) Explicit archive set from `cargo build --message-format=json`                          | Task 3 (parse + resolve, 7 unit tests), Task 5 (wire-in)                                                                                         |
| (c) `ranlib` normalization before `libtool`                                                | Task 4 (port + unit test), Task 5 (wire-in)                                                                                                      |
| (d) Deployment target: verify, extend only if needed; do not redo WS3                      | Task 7 (verification-only, with a contingency branch)                                                                                            |
| KEY GATE: `build-rustty.sh` succeeds with NO Homebrew gettext                              | Task 6 (surgical Cellar rename + trap-restored, with a proof that `-lintl` really stopped linking)                                               |
| `nm`/`otool` symbol assertions                                                             | Steps 1.9, 5.3, 5.5, 6.4, 6.5                                                                                                                    |
| Both-platform gate                                                                         | Steps 8.1-8.2                                                                                                                                    |
| Ledger strikes (§2.3-2.5, MILESTONE, DEFERRALS) + WS4-M1 resolved note                     | Task 8                                                                                                                                           |
| Branch/merge flow per roadmap                                                              | Task 0, Step 8.10                                                                                                                                |
| Oracle mirroring (SharedDeps/GhosttyLib/CombineArchivesStep/LibtoolStep)                   | Task 1 (`build.zig` `srcs` + flags + include layering), Task 3 (`GhosttyLib.zig` enumerated deps), Task 4 (`LibtoolStep.zig` `normalizeArchive`) |
| Existing `-sys` vendoring pattern followed                                                 | Task 1 keeps `cc` + `bindgen` + `links` exactly as `zlib-sys`/`oniguruma-sys`/`libpng-sys` do                                                    |

No gaps.

**2. Placeholder scan.** No "TBD"/"TODO"/"implement later"/"similar to Task N"/"add error handling" text. Every code step carries complete code; every command step carries the exact command and expected output. The one conditional step (7.3) states its trigger condition and the full replacement code for that branch.

**3. Type consistency.** `parse_artifacts` / `ParsedArtifacts { staticlibs, native }` / `NativeArchiveRequest { package_id, lib_name, search_paths }` / `resolve_with(&dyn Fn(&Path) -> bool)` / `resolve()` / `normalize_archive(&Path, &Path)` / `cargo_build_json(&Path, &[&str], &[(&str, &str)])` are declared in Task 3/4 and used with identical names and signatures in Task 5. The archive name `rustty_intl` is introduced in Task 1 (Step 1.6) and used identically in Steps 1.9, 2.3's comment, and the Task 8 ledger text. `gettext_sys::catalog::{bindtextdomain, textdomain, dgettext}` is defined in Step 1.1 and used only in Step 1.1's own test; `crates/rustty-os/src/i18n.rs` deliberately keeps its own extern block (stated in Task 2's Interfaces). The gate script's paths (`target/aarch64-apple-darwin/xcframework/libRusttyKit.a`, `/tmp/ws9-gate-build.log`) match those used in Tasks 5 and 7.
