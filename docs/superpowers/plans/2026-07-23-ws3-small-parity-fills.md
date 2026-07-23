# WS3 Small Parity Fills Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close four small parity gaps: (A) the kitty-graphics PNG-decode default hook (`rustty-vt/sys.rs` → `rustty-wuffs::png::decode`), (B) the three `+version` STUB lines (renderer / libxev / toolchain), (C) the `host_view()` iOS mapping verification + non-darwin compile proof, and (D) the macOS deployment-target warning verification — leaving every touched ledger truthful.

**Architecture:** (A) adds an off-by-default `wuffs` cargo feature to `rustty-vt` that installs `decode_png_wuffs` as the `sys::decode_png` default (mirroring ghostty `sys.zig`'s comptime `artifact == .lib ? null : &decodePngWuffs`), enabled by `rustty-core` (the SharedDeps `.ghostty` analog) so every application-tier consumer gets it while the standalone `rustty-vt-c` lib keeps the reference `null` default. (B) captures the rustc version in a new `rustty-build-config` build script, adds a cross-platform `rustty_xev::backend()` accessor, and maps `build_config::renderer()` to the reference `@typeName` strings in `version.rs`. (C) and (D) are mostly verification: the code fixes already landed (stale ledgers) — we add regression KATs, run the explicit checks, and strike the ledger entries.

**Tech Stack:** Rust 1.93.1 (pinned), cargo workspace at `/Users/gael/projects/rustty`, ghostty oracle at `/Users/gael/projects/ghostty` (pinned `4749c4e`, reference-only), Fedora aarch64 VM reachable as `ssh rvm`, Xcode 26.5 (`xcodebuild`, `otool`, `libtool`).

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

| File | Responsibility |
|---|---|
| `crates/rustty-vt/Cargo.toml` | new `wuffs` feature + optional `rustty-wuffs` dep |
| `crates/rustty-vt/src/sys.rs` | `decode_png_wuffs` body + artifact-keyed default for the `decode_png` global + unit tests |
| `crates/rustty-vt/src/stream_terminal.rs` | Stream-level kitty-PNG-transmit KAT |
| `crates/rustty-core/Cargo.toml` | enables `rustty-vt/wuffs` for the application tier |
| `crates/rustty-build-config/build.rs` | NEW — captures `$RUSTC --version` |
| `crates/rustty-build-config/src/lib.rs` | `RUSTC_VERSION` const + test |
| `crates/rustty-xev/src/lib.rs` | cross-platform `backend()` accessor |
| `crates/rustty-cli/Cargo.toml` | adds `rustty-xev` dep |
| `crates/rustty-cli/src/version.rs` | replaces the 3 STUB lines with real values |
| `crates/rustty/tests/cli_smoke.rs` | `+version` KAT asserts the real line content |
| `crates/rustty-apprt/src/embedded.rs` | `platform_host_view` helper extraction + regression tests |
| `xtask/src/main.rs` | (contingency only) CFLAGS deployment-target forwarding |
| `.port/phase-2/DEFERRALS.md`, `.port/phase-2/INTEGRATION_PARITY.md`, `macos/MILESTONE.md` | ledger strikes |

**Pre-verified facts the tasks rely on** (researched 2026-07-23, do not re-derive):

- Oracle `src/terminal/sys.zig`: `pub var decode_png: ?DecodePngFn = png: { if (build_options.artifact == .lib) break :png null; break :png &decodePngWuffs; };` — and `terminalOptions(.lib)` is used ONLY by the standalone libghostty-vt (`src/build/GhosttyZig.zig:67`); every app artifact including the macOS GhosttyKit uses `.ghostty` (`src/build/SharedDeps.zig:136`). So rustty-apprt-c (the xcframework lib, which deps rustty-core) SHOULD get the wuffs default; rustty-vt-c (the standalone vt C lib) should NOT.
- `crates/rustty-vt/src/options.rs:32`: `pub const ARTIFACT: Artifact = Artifact::Ghostty;` (`Artifact` = `crate::build_options::Artifact`, variants `Ghostty`/`Lib`, `build_options.rs:214-220`).
- `rustty-wuffs` API: `pub fn decode<A: Allocator + Copy>(alloc: A, data: &[u8]) -> Result<ImageData<A>, Error>` (`crates/rustty-wuffs/src/png.rs:36`); `Error` variants `OutOfMemory | WuffsError | Overflow` (re-exported at crate root); `ImageData::into_raw(self) -> (u32, u32, NonNull<u8>, usize)` transfers ownership of an align-1 allocation of exactly `len` bytes over `A` (`lib.rs:66-75`). `allocator_api2::alloc::Global` forwards to the std global allocator, so `Vec::from_raw_parts(ptr, len, len)` is layout-compatible.
- `rustty-vt` already deps `allocator-api2` (`Cargo.toml:87`); `rustty-core` already enables vt features (`rustty-vt = { workspace = true, features = ["oniguruma"] }`, `crates/rustty-core/Cargo.toml:25`); the workspace `Cargo.toml:242` already has `rustty-wuffs = { path = "crates/rustty-wuffs" }`.
- PNG fixture: `crates/rustty-vt/src/kitty/testdata/image-png-none-50x76-2147483647-raw.data` (86 bytes, 50×76). Its base64 is `iVBORw0KGgoAAAANSUhEUgAAADIAAABMAQMAAAAV2XZDAAAAA1BMVEUrLzc5wwm5AAAADklEQVR4AWOgIRgFowAAAmAAATnED6oAAAAASUVORK5CYII=`.
- Kitty image storage: `unsafe { &(*terminal.screens.active).kitty_images }` (the `graphics_exec.rs:172` idiom); `ImageStorage::image_by_id(&self, u32) -> Option<Image>`; `kitty::graphics_image::Image` has pub `width: u32`, `height: u32`, `format: command::Format`.
- Oracle `cli/version.zig` prints `renderer.Renderer` with `{}` (= `@typeName`); `src/renderer.zig:38-42`: `.metal => GenericRenderer(Metal)`, `.opengl => GenericRenderer(OpenGL)`, `.webgl => WebGL` — type names `renderer.generic.Renderer(renderer.Metal)` / `renderer.generic.Renderer(renderer.OpenGL)` / `renderer.WebGL`. `xev.backend` prints with `{t}` (= tag name: `kqueue`/`epoll`/`io_uring`/…).
- `rustty-xev`: macOS is the flat static kqueue Api (no Dynamic layer — `Backend::candidates()` is `&[Kqueue]`); Linux has `#[cfg(target_os = "linux")] mod dynamic;` (private) with `pub fn backend() -> Backend` reading the runtime-detected global (`dynamic.rs:163`, default `Epoll` until `detect()` runs — `detect()` wiring is WS4's `state.init`, out of scope here).
- `crates/rustty-apprt/src/embedded.rs:2795` `host_view()` ALREADY contains the iOS arm and the non-darwin cfg-gating — INTEGRATION_PARITY §2.6/§2.9 are stale. WS3 adds the regression tests, runs the explicit non-darwin build, and strikes the entries.
- `xtask/src/main.rs:301/335` ALREADY pins `MACOS_DEPLOYMENT_TARGET = "13.0"` as an env on the xcframework cargo build — `macos/MILESTONE.md:55` is stale pending verification. WS3 verifies with a clean rebuild + `otool` + `build-rustty.sh` log grep, and strikes the entry.
- Acyclic gate: `bash .port/tools/gates/check_acyclic.sh` from the repo root.

## Task 0: Branch

- [ ] **Step 0.1: Create the WS3 branch off up-to-date main**

Run:
```bash
cd /Users/gael/projects/rustty
git checkout main && git pull && git checkout -b ws3-small-parity-fills
```
Expected: `Switched to a new branch 'ws3-small-parity-fills'`.

---

## Task 1: rustty-vt `wuffs` feature — the PNG-decode default hook (item A, core)

**Files:**
- Modify: `crates/rustty-vt/Cargo.toml` (features block ~line 30; `[dependencies]` block)
- Modify: `crates/rustty-vt/src/sys.rs` (whole seam: lines 20-25 header note, 52-58 default, 72-97 commented body)
- Test: `crates/rustty-vt/src/sys.rs` (new `mod tests`), `crates/rustty-vt/src/stream_terminal.rs` (new KAT in the existing `mod tests`, after `basic_print`)

**Interfaces:**
- Consumes: `rustty_wuffs::png::decode`, `rustty_wuffs::Error`, `rustty_wuffs::ImageData::into_raw` (signatures in "Pre-verified facts").
- Produces: `rustty-vt` cargo feature `wuffs`; `sys::decode_png` default `Some(decode_png_wuffs)` under that feature (still `None` without it, and for a hypothetical `Artifact::Lib`). Task 2 enables the feature from `rustty-core`.

- [ ] **Step 1.1: Add the feature + optional dep to `crates/rustty-vt/Cargo.toml`**

In the `[features]` section (after the `simd = []` line):

```toml
# Whether the wuffs image-decode binding is available (the reference
# `@import("wuffs")` inside sys.zig's `decodePngWuffs`). The reference default
# is comptime: `artifact == .lib` -> null, else `&decodePngWuffs` — and ONLY the
# standalone libghostty-vt build passes `.lib` (GhosttyZig.zig:67); every app
# artifact incl. the macOS embedded lib uses `.ghostty` (SharedDeps.zig:136).
# Mapped as a cargo feature: rustty-core (the SharedDeps analog) turns it ON for
# the whole application tier; rustty-vt-c (the GhosttyZig `.lib` analog) leaves
# it OFF so a `-p rustty-vt-c` build keeps the reference `null` default.
# (Workspace-wide dev builds unify the feature onto rustty-vt-c too — additive
# and harmless; the SHIPPED standalone vt lib is built `-p rustty-vt-c`.)
wuffs = ["dep:rustty-wuffs"]
```

In `[dependencies]` (next to the other optional deps):

```toml
# The pkg/wuffs safe-decode parity crate — `decodePngWuffs`'s `wuffs.png.decode`
# (sys.zig). Optional: only the `wuffs` feature (application artifacts) pulls it;
# a strictly-earlier leaf (deps: wuffs-sys + allocator-api2), no cycle (R4).
rustty-wuffs = { workspace = true, optional = true }
```

- [ ] **Step 1.2: Write the failing unit tests in `crates/rustty-vt/src/sys.rs`**

Append at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// LIFETIMES-AUDIT #2 un-defer: the reference comptime default
    /// (sys.zig:18-24) is `&decodePngWuffs` for every non-`.lib` artifact and
    /// `null` otherwise. This crate's `options::ARTIFACT` is `Ghostty`
    /// (options.rs:32), so with the `wuffs` feature (the application tier) the
    /// default MUST be installed; without it (the vt-c `.lib` analog) it stays
    /// `None`.
    #[test]
    fn default_decoder_matches_artifact_and_feature() {
        let installed = decode_png.read().unwrap().is_some();
        assert_eq!(cfg!(feature = "wuffs"), installed);
    }

    /// The installed default actually decodes a real PNG: the kitty testdata
    /// 50x76 fixture -> 50*76*4 RGBA bytes (wuffs decodes to RGBA_NONPREMUL).
    #[cfg(feature = "wuffs")]
    #[test]
    fn default_decoder_decodes_png() {
        // Deref-copy out of the guard (`Option<fn>` is Copy — the
        // graphics_image.rs:720 `match *sys::decode_png.read().unwrap()` idiom).
        let f = (*decode_png.read().unwrap()).expect("wuffs default decoder installed");
        let img = f(include_bytes!(
            "kitty/testdata/image-png-none-50x76-2147483647-raw.data"
        ))
        .expect("PNG decodes");
        assert_eq!(50, img.width);
        assert_eq!(76, img.height);
        assert_eq!(50 * 76 * 4, img.data.len());
    }
}
```

- [ ] **Step 1.3: Write the failing Stream-level KAT in `crates/rustty-vt/src/stream_terminal.rs`**

Inside the existing `mod tests` (directly after the `basic_print` test):

```rust
    // WS3-A KAT (rustty addition, not a reference test): a kitty-graphics PNG
    // transmit sequence through the vt Stream decodes via the wuffs DEFAULT
    // hook — no embedder installed a decoder — proving the non-lib comptime
    // default (sys.zig:18-24) survives the port. `q=2` (quiet always) keeps the
    // handler from emitting an OK response to the (test) pty.
    #[cfg(feature = "wuffs")]
    #[test]
    fn kitty_graphics_png_transmit_decodes_via_default_hook() {
        // Base64 of kitty/testdata/image-png-none-50x76-2147483647-raw.data
        // (the `t=d` direct-medium payload is base64 per the kitty protocol).
        const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAADIAAABMAQMAAAAV2XZDAAAAA1BMVEUrLzc5wwm5AAAADklEQVR4AWOgIRgFowAAAmAAATnED6oAAAAASUVORK5CYII=";

        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // APC G: a=t (transmit), f=100 (PNG), t=d (direct), i=31, q=2.
            let seq = format!("\x1b_Ga=t,f=100,t=d,i=31,q=2;{PNG_B64}\x1b\\");
            s.next_slice(seq.as_bytes());
        }
        // The graphics_exec.rs:172 storage-access idiom.
        let img = unsafe { &(*t.screens.active).kitty_images }
            .image_by_id(31)
            .expect("transmitted image stored (PNG decoded by the default hook)");
        assert_eq!(50, img.width);
        assert_eq!(76, img.height);
        assert_eq!(img.format, kitty::graphics_command::Format::Rgba);
        t.deinit(Global);
    }
```

(If `kitty::graphics_command` is not already reachable from the test module's imports, it is — the module header has `use crate::{kitty, size};`.)

- [ ] **Step 1.4: Run the tests to verify they fail**

Run: `cargo test -p rustty-vt --features wuffs default_decoder 2>&1 | tail -20`
Expected: FAIL — `default_decoder_matches_artifact_and_feature` asserts `true == false` (default still `None`), and `default_decoder_decodes_png` panics on `expect("wuffs default decoder installed")`.

Run: `cargo test -p rustty-vt --features wuffs kitty_graphics_png_transmit 2>&1 | tail -10`
Expected: FAIL — `expect("transmitted image stored …")` panics (PNG format is disabled when the hook is `None`; the transmit is dropped).

- [ ] **Step 1.5: Implement `decode_png_wuffs` + the artifact-keyed default in `crates/rustty-vt/src/sys.rs`**

Replace the `pub static decode_png` declaration (lines 49-58) with:

```rust
/// Decode PNG data into RGBA pixels. If null, PNG decoding is unsupported
/// and the exact semantics are up to callers. For example, the Kitty Graphics
/// Protocol will work but cannot accept PNG images.
// PORT NOTE: the reference is a `pub var` with a comptime default:
//     pub var decode_png: ?DecodePngFn = png: {
//         if (build_options.artifact == .lib) break :png null;
//         break :png &decodePngWuffs;
//     };
// Mapped 1:1: `options::ARTIFACT` carries the artifact (options.rs:32,
// `Ghostty`; the `.lib` arm is kept for the later lib-artifact seam), and the
// wuffs binding rides the `wuffs` cargo feature (rustty-core — the SharedDeps
// `.ghostty` analog — enables it for the application tier; rustty-vt-c — the
// GhosttyZig `.lib` analog — does not, keeping the reference `null` default).
// We use a `RwLock` so the global is safely mutable across threads (the
// reference relies on Zig's `var` being mutated only at startup).
pub static decode_png: RwLock<Option<DecodePngFn>> = RwLock::new(DECODE_PNG_DEFAULT);

#[cfg(feature = "wuffs")]
const DECODE_PNG_DEFAULT: Option<DecodePngFn> =
    if matches!(crate::options::ARTIFACT, crate::build_options::Artifact::Lib) {
        None
    } else {
        Some(decode_png_wuffs as DecodePngFn)
    };
/// Without the wuffs binding no default decoder exists (embedders install one
/// at runtime via the swappable slot — the vt-c `set(.decode_png, ..)` path).
#[cfg(not(feature = "wuffs"))]
const DECODE_PNG_DEFAULT: Option<DecodePngFn> = None;
```

Replace the commented-out `decodePngWuffs` block (the `// fn decodePngWuffs(…)` comment through the trailing `// PORT NOTE (DEFERRED SEAM)` paragraph, lines 72-97) with the live body, keeping the reference text as line-comments:

```rust
// fn decodePngWuffs(
//     alloc: Allocator,
//     data: []const u8,
// ) DecodeError!Image {
#[cfg(feature = "wuffs")]
fn decode_png_wuffs(data: &[u8]) -> Result<Image, DecodeError> {
    // const wuffs = @import("wuffs");
    use allocator_api2::alloc::Global;
    // const result = wuffs.png.decode(
    //     alloc,
    //     data,
    // ) catch |err| switch (err) {
    //     error.WuffsError => return error.InvalidData,
    //     error.OutOfMemory => return error.OutOfMemory,
    //     error.Overflow => return error.InvalidData,
    // };
    let result = match rustty_wuffs::png::decode(Global, data) {
        Ok(result) => result,
        Err(rustty_wuffs::Error::WuffsError) => return Err(DecodeError::InvalidData),
        Err(rustty_wuffs::Error::OutOfMemory) => return Err(DecodeError::OutOfMemory),
        Err(rustty_wuffs::Error::Overflow) => return Err(DecodeError::InvalidData),
    };

    // return .{
    //     .width = result.width,
    //     .height = result.height,
    //     .data = result.data,
    // };
    //
    // PORT NOTE: the reference moves the alloc-owned `[]u8` into `Image.data`;
    // here ownership transfers via `into_raw` into the `Vec` that carries it
    // (sys.rs `Image.data: Vec<u8>`).
    let (width, height, ptr, len) = result.into_raw();
    // SAFETY: `into_raw` yields sole ownership of a `Global` (std-allocator)
    // align-1 allocation of exactly `len` bytes, matching `Vec<u8>`'s
    // `Layout::array::<u8>(len)` free layout with capacity == len.
    let data = unsafe { Vec::from_raw_parts(ptr.as_ptr(), len, len) };
    Ok(Image {
        width,
        height,
        data,
    })
}
```

Also update the module doc header (lines 20-25): replace the "the default value is `None` here … deferred stub" sentences with:

```
//! The default `decode_png` impl uses the `rustty-wuffs` binding under the
//! `wuffs` cargo feature (the reference `@import("wuffs")`); the default is
//! `None` only for the `.lib` artifact / featureless builds, exactly as the
//! reference comptime block reads.
```
and change `PORT STATUS: done` stays `done`.

- [ ] **Step 1.6: Run the tests to verify they pass**

Run: `cargo test -p rustty-vt --features wuffs default_decoder kitty_graphics_png_transmit 2>&1 | tail -10`
Expected: PASS (3 tests). If the Stream KAT still fails with the image missing, debug with `q` removed to see the error response — but the fixture and sequence above were verified against the parser's field grammar.

- [ ] **Step 1.7: Verify the previously-skipped reference test now runs, and the featureless build still skips**

Run: `cargo test -p rustty-vt --features wuffs image_load_png_not_compressed_regular_file 2>&1 | tail -5`
Expected: PASS (the graphics_image.rs:1255 skip-guard no longer triggers — it now exercises the real decode).

Run: `cargo test -p rustty-vt default_decoder_matches_artifact_and_feature 2>&1 | tail -5`
Expected: PASS (featureless: default stays `None`).

Run: `cargo test -p rustty-vt-c 2>&1 | tail -5`
Expected: PASS unchanged (the vt-c `set decode_png with null clears` / `installs wrapper` tests manage the slot explicitly; `-p rustty-vt-c` builds vt WITHOUT the feature so its start state is still `None`).

- [ ] **Step 1.8: Commit**

```bash
git add crates/rustty-vt/Cargo.toml crates/rustty-vt/src/sys.rs crates/rustty-vt/src/stream_terminal.rs
git commit -m "rustty-vt: wuffs decode_png default hook — non-lib PNG parity (WS3-A)"
```

---

## Task 2: Application-tier wiring + ledger strike (item A, finish)

**Files:**
- Modify: `crates/rustty-core/Cargo.toml:25` (the `rustty-vt` dep line + its comment at :22-24)
- Modify: `.port/phase-2/DEFERRALS.md` (~line 4491, the "### 2. rustty-vt `sys.rs` decode_png" LIFETIMES-AUDIT entry)

**Interfaces:**
- Consumes: the `wuffs` feature from Task 1.
- Produces: every app-tier consumer (rustty-core → rustty-termio path, rustty-apprt, rustty-apprt-c/xcframework, the `rustty` bin) compiles rustty-vt with the wuffs default installed.

- [ ] **Step 2.1: Enable the feature in `crates/rustty-core/Cargo.toml`**

Change line 25 from:
```toml
rustty-vt = { workspace = true, features = ["oniguruma"] }
```
to:
```toml
rustty-vt = { workspace = true, features = ["oniguruma", "wuffs"] }
```
and extend the comment block above it (lines ~22-24) with one line:
```toml
# `wuffs` = the sys.zig decodePngWuffs default for the app tier (SharedDeps
# `.ghostty` terminalOptions analog; the standalone rustty-vt-c stays `.lib`/None).
```

- [ ] **Step 2.2: Verify the workspace + acyclic gate**

Run: `cargo build --workspace 2>&1 | tail -3`
Expected: `Finished` with no errors.

Run: `bash .port/tools/gates/check_acyclic.sh`
Expected: GREEN (rustty-wuffs is a strictly-earlier leaf; no new cycle).

- [ ] **Step 2.3: Strike the DEFERRALS entry**

In `.port/phase-2/DEFERRALS.md`, find the heading (currently ~line 4491):
```
### 2. rustty-vt `sys.rs` decode_png — non-lib default decoder parity (LOW, fidelity gap)
```
Change it to:
```
### 2. rustty-vt `sys.rs` decode_png — non-lib default decoder parity — ✅ RESOLVED (2026-07-23, WS3)
```
and append at the end of that entry's paragraph (after the "**Un-defer trigger:** …" sentence):
```
**RESOLVED (2026-07-23, `ws3-small-parity-fills`):** `decodePngWuffs` is live in
sys.rs behind the `wuffs` cargo feature (optional `rustty-wuffs` dep); the
default is the faithful comptime map — `Artifact::Lib` (or featureless) → None,
else `Some(decode_png_wuffs)`. rustty-core enables the feature for the app tier
(SharedDeps `.ghostty` analog); rustty-vt-c keeps the `.lib` None default.
KATs: sys.rs `default_decoder_*` + stream_terminal.rs
`kitty_graphics_png_transmit_decodes_via_default_hook` (run with
`--features wuffs`); graphics_image.rs `image_load_png_not_compressed_regular_file`
no longer skips under the feature.
```

- [ ] **Step 2.4: Commit**

```bash
git add crates/rustty-core/Cargo.toml .port/phase-2/DEFERRALS.md
git commit -m "rustty-core: enable rustty-vt/wuffs for the app tier; strike DEFERRALS decode_png entry (WS3-A)"
```

---

## Task 3: `rustty-build-config` rustc-version capture (item B, part 1)

**Files:**
- Create: `crates/rustty-build-config/build.rs`
- Modify: `crates/rustty-build-config/src/lib.rs` (add `RUSTC_VERSION` near `VERSION_STRING`, ~line 140; add a test)

**Interfaces:**
- Produces: `rustty_build_config::RUSTC_VERSION: &'static str` — e.g. `"1.93.1 (xxxxxxxxx 2026-xx-xx)"` (the `rustc ` prefix stripped, mirroring `builtin.zig_version_string` being a bare version). Task 5 prints it.

- [ ] **Step 3.1: Write the failing test**

In `crates/rustty-build-config/src/lib.rs`, add to the existing `#[cfg(test)] mod tests` (create one at file end if absent):

```rust
    /// WS3-B: the toolchain version captured by build.rs. The workspace pins
    /// rustc via rust-toolchain.toml, so the string starts with a 1.x version
    /// and carries no "rustc " prefix (mirrors `builtin.zig_version_string`
    /// being a bare version string).
    #[test]
    fn rustc_version_is_captured() {
        assert!(
            crate::RUSTC_VERSION.starts_with("1."),
            "RUSTC_VERSION should be a bare version, got: {:?}",
            crate::RUSTC_VERSION
        );
        assert!(!crate::RUSTC_VERSION.contains("rustc "));
    }
```

- [ ] **Step 3.2: Run to verify it fails**

Run: `cargo test -p rustty-build-config rustc_version 2>&1 | tail -5`
Expected: FAIL to compile — `RUSTC_VERSION` not found.

- [ ] **Step 3.3: Implement**

Create `crates/rustty-build-config/build.rs`:

```rust
//! build.rs — capture the Rust toolchain version at compile time.
//!
//! Reference analog: Zig exposes `builtin.zig_version_string` to every module;
//! `cli/version.zig` prints it as the toolchain line. Rust has no builtin, so
//! this build script asks the ACTUAL compiler cargo is driving (`$RUSTC`,
//! falling back to `rustc` on PATH) and bakes the answer into the crate env.

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let out = std::process::Command::new(&rustc)
        .arg("--version")
        .output()
        .expect("failed to run `rustc --version`");
    let full = String::from_utf8(out.stdout).expect("rustc --version is UTF-8");
    // "rustc 1.93.1 (hash date)" -> "1.93.1 (hash date)" (a bare version
    // string, mirroring `builtin.zig_version_string`).
    let trimmed = full.trim();
    let version = trimmed.strip_prefix("rustc ").unwrap_or(trimmed);
    println!("cargo:rustc-env=RUSTTY_RUSTC_VERSION={version}");
    println!("cargo:rerun-if-env-changed=RUSTC");
}
```

In `crates/rustty-build-config/src/lib.rs`, after the `VERSION_STRING` const (~line 141):

```rust
/// The Rust toolchain version this build was compiled with (bare version, e.g.
/// `1.93.1 (…)`).
///
/// Reference analog: `builtin.zig_version_string` — the reference has no
/// build_config member for this (Zig provides it as a builtin); it lives here
/// because build-config is the port's home for build-environment constants.
/// Captured by `build.rs` from `$RUSTC --version`.
pub const RUSTC_VERSION: &str = env!("RUSTTY_RUSTC_VERSION");
```

- [ ] **Step 3.4: Run to verify it passes**

Run: `cargo test -p rustty-build-config rustc_version 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 3.5: Commit**

```bash
git add crates/rustty-build-config/build.rs crates/rustty-build-config/src/lib.rs
git commit -m "rustty-build-config: capture RUSTC_VERSION via build.rs (WS3-B toolchain line)"
```

---

## Task 4: `rustty_xev::backend()` cross-platform accessor (item B, part 2)

**Files:**
- Modify: `crates/rustty-xev/src/lib.rs` (directly after `pub use backend::Backend;`, line 73)
- Test: same file's additions carry `#[cfg(test)]` tests (or append to an existing lib-level tests mod if one exists at file end — keep the new tests self-contained either way)

**Interfaces:**
- Produces: `rustty_xev::backend() -> rustty_xev::Backend` on macOS (const `Kqueue`) and Linux (the Dynamic runtime global). Task 5 consumes it.

- [ ] **Step 4.1: Write the failing test**

Append to `crates/rustty-xev/src/lib.rs`:

```rust
#[cfg(test)]
mod backend_accessor_tests {
    /// WS3-B: `xev.backend` — macOS is the flat static kqueue Api
    /// (`pub const backend = be;` in libxev main.zig's Xev(be, T)), so the
    /// accessor is the comptime `.kqueue`.
    #[cfg(target_os = "macos")]
    #[test]
    fn backend_is_kqueue_on_macos() {
        assert_eq!(crate::Backend::Kqueue, crate::backend());
    }

    /// WS3-B: on Linux the accessor is the Dynamic layer's runtime global
    /// (dynamic.zig:91) — always one of the Linux candidates.
    #[cfg(target_os = "linux")]
    #[test]
    fn backend_is_a_linux_candidate() {
        let be = crate::backend();
        assert!(matches!(
            be,
            crate::Backend::IoUring | crate::Backend::Epoll
        ));
    }
}
```

- [ ] **Step 4.2: Run to verify it fails**

Run: `cargo test -p rustty-xev backend_accessor 2>&1 | tail -5`
Expected: FAIL to compile — `crate::backend` not found.

- [ ] **Step 4.3: Implement**

In `crates/rustty-xev/src/lib.rs`, directly after `pub use backend::Backend;` (line 73):

```rust
/// The backend in use (the reference `xev.backend` read).
///
/// PORT NOTE: on macOS the crate surface IS the static kqueue `Api()`
/// (XEV_BLUEPRINT §0 — `Backend::candidates()` is the single-element
/// `&[kqueue]`, comptime-eliding the Dynamic layer), and libxev's `Xev(be, T)`
/// declares `pub const backend = be;` — so this is the comptime `.kqueue`.
#[cfg(target_os = "macos")]
pub fn backend() -> Backend {
    Backend::Kqueue
}
```

and add to the Linux export block (next to the other `#[cfg(target_os = "linux")] pub use dynamic::…` re-exports; if the dynamic re-exports live elsewhere, place this immediately after `mod dynamic;`):

```rust
/// The backend in use (the reference `xev.backend` read) — the Dynamic layer's
/// runtime-detected global (dynamic.zig:91; default epoll until `detect()`).
#[cfg(target_os = "linux")]
pub use dynamic::backend;
```

- [ ] **Step 4.4: Run to verify it passes**

Run: `cargo test -p rustty-xev backend_accessor 2>&1 | tail -5`
Expected: PASS (macOS arm here; the Linux arm passes in the Task 8 VM gate).

- [ ] **Step 4.5: Commit**

```bash
git add crates/rustty-xev/src/lib.rs
git commit -m "rustty-xev: cross-platform backend() accessor for xev.backend reads (WS3-B)"
```

---

## Task 5: `+version` real lines + KAT + ledger strike (item B, finish)

**Files:**
- Modify: `crates/rustty-cli/Cargo.toml` (add `rustty-xev` dep, next to `rustty-build-config` ~line 46)
- Modify: `crates/rustty-cli/src/version.rs` (drop the 3 STUB consts; real lines)
- Test: `crates/rustty/tests/cli_smoke.rs` (`version_plus`, line 70)
- Modify: `.port/phase-2/DEFERRALS.md` (~lines 2602-2617, the C1 `version.zig` PARTIAL bullet)

**Interfaces:**
- Consumes: `rustty_build_config::RUSTC_VERSION` (Task 3), `rustty_xev::backend()` + `rustty_xev::Backend` (Task 4), `rustty_build_config::renderer()` → `rustty_build_config::renderer::Backend::{Metal, Opengl, Webgl}` (pre-existing).
- Produces: `+version` output lines `  - Rust version  : 1.93.1 (…)`, `  - renderer      : renderer.generic.Renderer(renderer.Metal|renderer.OpenGL)`, `  - libxev        : kqueue|epoll|io_uring`.

- [ ] **Step 5.1: Extend the KAT first (failing)**

In `crates/rustty/tests/cli_smoke.rs`, replace the `version_plus` test (lines 68-79) with:

```rust
// `rustty +version` -> exit 0, stdout starts `Rustty ` (the title-case product
// name). WS3-B: the renderer / libxev / toolchain lines now carry REAL values
// (ADV-FIX 1 retired) — assert their content and the absence of every stub.
#[test]
fn version_plus() {
    let out = run(&["+version"], &[]);
    assert!(out.status.success(), "+version should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("Rustty "),
        "+version stdout should start with `Rustty `, got: {:?}",
        stdout
    );
    assert!(
        !stdout.contains("STUB") && !stdout.contains("deferred"),
        "+version must carry no stub lines, got: {stdout}"
    );
    assert!(
        stdout.contains("  - Rust version  : 1."),
        "toolchain line missing/wrong: {stdout}"
    );
    #[cfg(target_os = "macos")]
    {
        assert!(
            stdout.contains("  - renderer      : renderer.generic.Renderer(renderer.Metal)"),
            "renderer line missing/wrong: {stdout}"
        );
        assert!(
            stdout.contains("  - libxev        : kqueue"),
            "libxev line missing/wrong: {stdout}"
        );
    }
    #[cfg(target_os = "linux")]
    {
        assert!(
            stdout.contains("  - renderer      : renderer.generic.Renderer(renderer.OpenGL)"),
            "renderer line missing/wrong: {stdout}"
        );
        assert!(
            stdout.contains("  - libxev        : epoll")
                || stdout.contains("  - libxev        : io_uring"),
            "libxev line missing/wrong: {stdout}"
        );
    }
}
```

- [ ] **Step 5.2: Run to verify it fails**

Run: `cargo test -p rustty --test cli_smoke version_plus 2>&1 | tail -8`
Expected: FAIL on the `no stub lines` assertion (current output contains `(deferred: rustty-renderer — Phase-3 / R-d)` etc.).

- [ ] **Step 5.3: Add the `rustty-xev` dep to `crates/rustty-cli/Cargo.toml`**

Next to the `rustty-build-config` dep (~line 46), and UPDATE the stale comment above it (lines ~43-45 currently say "so no rustty-renderer / rustty-xev dep is needed" — rewrite to):

```toml
# `+version` build/version constants + the product name via rustty-identity
# (R2). The renderer line derives from build_config::renderer() (no
# rustty-renderer edge needed — the renderer.zig selection switch is data);
# the libxev line reads rustty-xev's `backend()` (the reference `xev.backend`).
rustty-build-config = { workspace = true }
rustty-identity = { workspace = true }
rustty-xev = { workspace = true }
```

- [ ] **Step 5.4: Rewrite the stub lines in `crates/rustty-cli/src/version.rs`**

1. Update the module header (lines 5-10): replace the `PORT STATUS: PARTIAL …` paragraph with:

```rust
//! PORT STATUS: done (WS3-B). The renderer / libxev / toolchain lines carry
//! real values: `build_config::renderer()` mapped to the reference `@typeName`
//! strings, `rustty_xev::backend()` (the reference `xev.backend`), and
//! `build_config::RUSTC_VERSION` (the `builtin.zig_version_string` analog).
//! Still deferred (unchanged): the OSC-8 commit hyperlink (`commit_hash()` is
//! always None — no `+BUILD` metadata / no repo-URL identity constant yet).
```

2. Update the import comments (lines 19-20):

```rust
// const xev = @import("../global.zig").xev;          -> rustty_xev (WS3-B)
// const renderer = @import("../renderer.zig");        -> renderer_type_name() (WS3-B)
```

3. DELETE the three stub consts (`RENDERER_STUB` lines 32-36, `LIBXEV_STUB` lines 38-40, `TOOLCHAIN_VERSION_STUB` lines 42-46) and add in their place:

```rust
/// The renderer type name, exactly as the reference prints it: `{}` on the
/// comptime type `renderer.Renderer` is `@typeName`, and renderer.zig:38-42
/// selects `.metal => GenericRenderer(Metal)`, `.opengl => GenericRenderer(OpenGL)`,
/// `.webgl => WebGL`. Derived from `build_config::renderer()` — the SAME
/// `build_config.renderer` the reference switch reads — so the CLI keeps no
/// rustty-renderer edge.
fn renderer_type_name() -> &'static str {
    match build_config::renderer() {
        build_config::renderer::Backend::Metal => {
            "renderer.generic.Renderer(renderer.Metal)"
        }
        build_config::renderer::Backend::Opengl => {
            "renderer.generic.Renderer(renderer.OpenGL)"
        }
        build_config::renderer::Backend::Webgl => "renderer.WebGL",
    }
}

/// The `{t}` tag name of the libxev backend (the reference `@tagName` of
/// `xev.backend`).
fn libxev_backend_tag() -> &'static str {
    match rustty_xev::backend() {
        rustty_xev::Backend::IoUring => "io_uring",
        rustty_xev::Backend::Epoll => "epoll",
        rustty_xev::Backend::Kqueue => "kqueue",
        rustty_xev::Backend::WasiPoll => "wasi_poll",
        rustty_xev::Backend::Iocp => "iocp",
    }
}
```

4. Replace the three output lines:

Line 124 (`  - Rust version`):
```rust
    // try stdout.print("  - Zig version   : {s}\n", .{builtin.zig_version_string});
    //   PORT NOTE: the Rust port's toolchain is rustc — the analogous version,
    //   captured at compile time (build_config::RUSTC_VERSION).
    writeln!(stdout, "  - Rust version  : {}", build_config::RUSTC_VERSION)?;
```

Lines 143-146 (renderer + libxev):
```rust
    // try stdout.print("  - renderer      : {}\n", .{renderer.Renderer});
    writeln!(stdout, "  - renderer      : {}", renderer_type_name())?;
    // try stdout.print("  - libxev        : {t}\n", .{xev.backend});
    writeln!(stdout, "  - libxev        : {}", libxev_backend_tag())?;
```

(The GTK comptime block comment, lines 147-181, stays as-is — it is a GTK-runtime line-set keyed on `app_runtime == .gtk` and remains out of WS3 scope; note it in the header if desired.)

- [ ] **Step 5.5: Run the KAT to verify it passes**

Run: `cargo test -p rustty --test cli_smoke version_plus 2>&1 | tail -5`
Expected: PASS.

Run: `cargo run -q -p rustty -- +version`
Expected output (macOS) includes:
```
  - Rust version  : 1.93.1 (…)
  - renderer      : renderer.generic.Renderer(renderer.Metal)
  - libxev        : kqueue
```

- [ ] **Step 5.6: Strike the DEFERRALS bullet**

In `.port/phase-2/DEFERRALS.md` (~2602-2617), the bullet beginning
```
- **`version.zig` is PARTIAL (ADV-FIX 1) — `renderer.Renderer` + `xev.backend`
  STUBBED.**
```
Change the bold lead-in to:
```
- **`version.zig` is PARTIAL (ADV-FIX 1) — `renderer.Renderer` + `xev.backend`
  STUBBED. ✅ RESOLVED (2026-07-23, WS3-B) for the renderer/libxev/toolchain
  lines — real values via build_config::renderer() type-name map,
  rustty_xev::backend(), and build_config::RUSTC_VERSION; the cli_smoke
  `version_plus` KAT now ASSERTS their content. Still open: the OSC-8
  commit-hash hyperlink (no `+BUILD` metadata / repo-URL identity constant).**
```
(Leave the body text as the historical record.)

- [ ] **Step 5.7: Commit**

```bash
git add crates/rustty-cli/Cargo.toml crates/rustty-cli/src/version.rs crates/rustty/tests/cli_smoke.rs .port/phase-2/DEFERRALS.md
git commit -m "rustty-cli: +version real renderer/libxev/toolchain lines (WS3-B, ADV-FIX 1 retired)"
```

---

## Task 6: `host_view()` regression tests + non-darwin proof + ledger truth (item C)

The iOS-uiview mapping and the non-darwin cfg-gating ALREADY exist in
`crates/rustty-apprt/src/embedded.rs:2795-2804` (landed with the W-ATTACH parity
fix; INTEGRATION_PARITY §2.6/§2.9 are stale). This task pins them with tests,
runs the explicit non-darwin compile the ledger asks for, and updates the ledger.

**Files:**
- Modify: `crates/rustty-apprt/src/embedded.rs` (extract `platform_host_view` helper near the `Platform` defs ~line 745; `host_view` at :2795 delegates; tests at file end)
- Modify: `.port/phase-2/INTEGRATION_PARITY.md` (§2.6 header ~line 115, §2.9 header ~line 161, and the §6 recommendation items 6 and 9 at ~lines 312/319)

**Interfaces:**
- Produces: `pub(crate) fn platform_host_view(platform: &Platform) -> *mut c_void` in `rustty_apprt::embedded` (same-module helper; the `Surface::host_view` trait method delegates to it).

- [ ] **Step 6.1: Write the failing tests**

Append to `crates/rustty-apprt/src/embedded.rs`:

```rust
#[cfg(test)]
mod host_view_tests {
    use super::*;

    /// INTEGRATION_PARITY §2.6 regression: the iOS arm maps `uiview` (the
    /// reference Metal.zig:99-102 `switch { .macos => v.nsview, .ios => v.uiview }`)
    /// — never the old null catch-all.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    #[test]
    fn ios_platform_maps_uiview() {
        let marker = 0xA5A5usize as *mut c_void;
        let platform = Platform::IOS(IOS { uiview: marker });
        assert_eq!(marker, platform_host_view(&platform));
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    #[test]
    fn macos_platform_maps_nsview() {
        let marker = 0x5A5Ausize as *mut c_void;
        let platform = Platform::MacOS(MacOS { nsview: marker });
        assert_eq!(marker, platform_host_view(&platform));
    }

    /// INTEGRATION_PARITY §2.9 proof: on non-darwin `MacOS`/`IOS` are ZSTs and
    /// the helper compiles (and returns null — the platform never renders there).
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    #[test]
    fn non_darwin_returns_null() {
        assert!(platform_host_view(&Platform::MacOS(MacOS)).is_null());
        assert!(platform_host_view(&Platform::IOS(IOS)).is_null());
    }
}
```

- [ ] **Step 6.2: Run to verify it fails**

Run: `cargo test -p rustty-apprt host_view_tests 2>&1 | tail -5`
Expected: FAIL to compile — `platform_host_view` not found.

- [ ] **Step 6.3: Extract the helper**

In `crates/rustty-apprt/src/embedded.rs`, after the `IOS` struct definitions (~line 745, before `PlatformC`), add:

```rust
/// The host view pointer of a [`Platform`] value — the exact mapping the
/// reference `Metal.init` performs (Metal.zig:99-102: `switch { .macos =>
/// v.nsview, .ios => v.uiview }`; both darwin arms yield a real view). The
/// `nsview`/`uiview` fields only exist on darwin (`MacOS`/`IOS` are ZSTs
/// off-darwin, §2.9), so both concrete arms are `#[cfg]`-gated and a
/// non-darwin wildcard returns null (the platform never renders there).
/// Extracted from `Surface::host_view` so the §2.6/§2.9 mapping is unit-testable.
pub(crate) fn platform_host_view(platform: &Platform) -> *mut c_void {
    match platform {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Platform::MacOS(m) => m.nsview,
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Platform::IOS(i) => i.uiview,
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        _ => core::ptr::null_mut(),
    }
}
```

Then replace the `host_view` body at :2795-2804 (keeping its W-ATTACH doc comment, moving the `PORT NOTE (R1 parity)` paragraph onto the helper if you prefer a single home) with:

```rust
    fn host_view(&self) -> *mut c_void {
        platform_host_view(&self.platform)
    }
```

- [ ] **Step 6.4: Run to verify it passes (macOS)**

Run: `cargo test -p rustty-apprt host_view_tests 2>&1 | tail -5`
Expected: PASS (2 darwin tests).

- [ ] **Step 6.5: The explicit non-darwin check (§2.9's asked-for verification)**

Run:
```bash
git push -u origin ws3-small-parity-fills
ssh rvm 'cd ~/projects/rustty && git fetch && git checkout ws3-small-parity-fills && git pull && cargo build -p rustty-apprt && cargo test -p rustty-apprt host_view_tests 2>&1 | tail -5'
```
Expected: build succeeds; `non_darwin_returns_null` PASSES.

- [ ] **Step 6.6: Update INTEGRATION_PARITY.md**

- §2.6 header (line ~115): change to
  `### 2.6 [LOW] host_view() drops the iOS→uiview mapping … — ✅ FIXED (uiview arm restored pre-WS3; WS3 adds the platform_host_view helper + ios_platform_maps_uiview regression KAT)`
- §2.9 header (line ~161): change to
  `### 2.9 [LOW] host_view field access on Platform::MacOS may not compile on non-darwin — ✅ VERIFIED (WS3: cfg-gated arms; rvm build + non_darwin_returns_null KAT green)`
- §6 recommendation items 6 (~line 312) and 9 (~line 319): prefix each with `✅ DONE (WS3) — `.
- Also line ~29 ("One true R1 dropped-mapping (host_view nulls the iOS uiview) exists…"): append ` — since fixed; see §2.6.`

- [ ] **Step 6.7: Commit**

```bash
git add crates/rustty-apprt/src/embedded.rs .port/phase-2/INTEGRATION_PARITY.md
git commit -m "rustty-apprt/embedded: platform_host_view helper + iOS/non-darwin KATs; truthful §2.6/§2.9 ledger (WS3-C)"
```

---

## Task 7: Deployment-target verification + ledger truth (item D)

`xtask/src/main.rs:301` already pins `MACOS_DEPLOYMENT_TARGET = "13.0"` and sets
it as env on the xcframework cargo build (:335) — but `macos/MILESTONE.md:55`
still lists the warning as open and no clean-rebuild verification was recorded.
This task forces a from-clean restamp, proves every archive member targets 13.0,
proves the app link emits no warning, and strikes the ledger. (Contingency step
included for the case where a member still stamps a newer OS.)

**Files:**
- Modify (verification only, contingency edit if needed): `xtask/src/main.rs:303-345`
- Modify: `macos/MILESTONE.md:55-56` and `:77-79`

- [ ] **Step 7.1: Force a clean restamp of the xcframework profile**

Run:
```bash
rm -rf target/aarch64-apple-darwin/xcframework
cargo run -p xtask -- xcframework 2>&1 | tail -3
```
Expected: exits 0; `dist/RusttyKit.xcframework` exists. (This is a full release-profile rebuild of the staticlib tree — several minutes.)

- [ ] **Step 7.2: Assert every object in the combined archive targets 13.0**

Run:
```bash
otool -l dist/RusttyKit.xcframework/macos-arm64/libRusttyKit.a 2>/dev/null \
  | grep -B1 -A3 -E "LC_BUILD_VERSION|LC_VERSION_MIN_MACOSX" \
  | grep -E "^\s*(minos|version) " | sort | uniq -c
```
Expected: every line is `minos 13.0` (or `version 13.0` for any legacy `LC_VERSION_MIN_MACOSX` member). NO line with a 14.x/15.x/26.x value.

- [ ] **Step 7.3 (CONTINGENCY — only if Step 7.2 shows a newer stamp): forward the floor to every C compile explicitly**

If some member stamps the running OS, a `cc`-driven build script is not reading the env. In `xtask/src/main.rs`, on the same `Command::new("cargo")` builder (after the existing `.env("MACOSX_DEPLOYMENT_TARGET", …)` at :335), add:

```rust
        // Belt-and-braces: some cc-driven -sys builds honor CFLAGS but cache
        // objects keyed only on source; force the min-version flag explicitly.
        .env(
            "CFLAGS",
            format!("-mmacosx-version-min={MACOS_DEPLOYMENT_TARGET}"),
        )
```

Then re-run Steps 7.1-7.2. To find WHICH member was stale (guides the fix):
```bash
cd target && mkdir -p /tmp/rk && cd /tmp/rk && ar x /Users/gael/projects/rustty/dist/RusttyKit.xcframework/macos-arm64/libRusttyKit.a && for o in *.o; do v=$(otool -l "$o" | grep -m1 -E "minos|^\s*version " | awk '{print $2}'); [ "$v" != "13.0" ] && echo "$o $v"; done
```

- [ ] **Step 7.4: Full app build — the warning is gone**

Run:
```bash
bash macos/build-rustty.sh 2>&1 | tee /private/tmp/claude-501/-Users-gael-projects-rustty/4ae4ff76-65a5-423c-8692-da2f6a7506b2/scratchpad/ws3-build-rustty.log | tail -5
grep -c "was built for newer macOS version" /private/tmp/claude-501/-Users-gael-projects-rustty/4ae4ff76-65a5-423c-8692-da2f6a7506b2/scratchpad/ws3-build-rustty.log || true
```
Expected: the build succeeds and the grep count is `0`.

- [ ] **Step 7.5: Strike the MILESTONE ledger**

In `macos/MILESTONE.md`:
- Lines 55-56, change the bullet to:
  `- **Deployment target**: ✅ DONE (WS3, 2026-07-23) — xtask pins MACOSX_DEPLOYMENT_TARGET=13.0 on the xcframework cargo build; clean rebuild verified via otool (all members minos 13.0) and a warning-free build-rustty.sh xcodebuild log.`
- Line ~78-79 (the "P3 polish" list): remove "the deployment-target warning." from the trailing item (leave the other P3 items intact).

- [ ] **Step 7.6: Commit**

```bash
git add macos/MILESTONE.md xtask/src/main.rs
git commit -m "macos: verify 13.0 deployment-target stamping end-to-end; strike MILESTONE entry (WS3-D)"
```
(Omit `xtask/src/main.rs` from the add if Step 7.3 was not needed.)

---

## Task 8: Both-platform gate, final sweep, merge

- [ ] **Step 8.1: macOS full gate**

Run each; all must be green:
```bash
cargo build --workspace 2>&1 | tail -2
cargo test -p rustty-vt --features wuffs 2>&1 | tail -3
cargo test -p rustty-vt 2>&1 | tail -3
cargo test -p rustty-vt-c 2>&1 | tail -3
cargo test -p rustty-build-config -p rustty-xev -p rustty-apprt 2>&1 | tail -3
cargo test -p rustty --test cli_smoke 2>&1 | tail -3
bash .port/tools/gates/check_acyclic.sh
```
Expected: all PASS; acyclic gate GREEN.

- [ ] **Step 8.2: Linux (VM) full gate**

Run:
```bash
git push
ssh rvm 'cd ~/projects/rustty && git checkout ws3-small-parity-fills && git pull && cargo build --workspace 2>&1 | tail -2'
ssh rvm 'cd ~/projects/rustty && cargo test -p rustty-vt --features wuffs 2>&1 | tail -3'
ssh rvm 'cd ~/projects/rustty && cargo test -p rustty-xev -p rustty-apprt 2>&1 | tail -3'
ssh rvm 'cd ~/projects/rustty && cargo test -p rustty --test cli_smoke version 2>&1 | tail -3'
```
Expected: all PASS — in particular `version_plus` proves the Linux `+version` prints `renderer.generic.Renderer(renderer.OpenGL)` and `epoll`/`io_uring`, and `backend_is_a_linux_candidate` + `non_darwin_returns_null` run for real.

- [ ] **Step 8.3: Self-review sweep**

Re-read the four spec items (A-D) against the diff (`git diff main --stat`). Confirm: no `unimplemented!()` added, no gate weakened, every touched ledger entry truthful, comments carry the reference line-mapping style of their file.

- [ ] **Step 8.4: Merge per roadmap sequencing rule 2**

Use superpowers:finishing-a-development-branch. Default flow:
```bash
git checkout main && git merge --no-ff ws3-small-parity-fills -m "WS3: small parity fills — decode_png default, +version real lines, host_view KATs, deployment-target verify"
git push
git branch -d ws3-small-parity-fills && git push origin --delete ws3-small-parity-fills
ssh rvm 'cd ~/projects/rustty && git checkout main && git pull'
```
Also tick the `WS3 small parity fills` box in the roadmap's plan-authoring checklist if not already ticked, and mark WS3 done wherever the roadmap tracks execution.
