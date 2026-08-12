# W2D_WINDOWS_OS — F-001: restore the Windows (ConPTY) compile of the shared OS layer

Design for fixing finding **F-001** (`.port/codex-vet/STATE.json`): make

```
cargo check -p rustty-os -p rustty-vt -p rustty-vt-c --target x86_64-pc-windows-msvc
```

green on the macOS host, restoring the ConPTY path to **type-checked parity**
with the oracle. **Design only; no `crates/` edits from this doc's author.**

Reference (read-only): `/Users/gael/projects/ghostty` @ pin `6ad1fe7d`.
Vendored-std oracle (the SAME oracle `mach.rs`/`page_allocator.rs` already cite):
Zig 0.15.2 std, on this host at `/opt/homebrew/Cellar/zig@0.15/0.15.2/lib/zig/`
(cited below as `zstd:`). Zig 0.16.0 (ghostty's `minimum_zig_version`, on this
host at `/opt/homebrew/Cellar/zig/0.16.0_1/lib/zig/`) was diffed against 0.15.2
for every cited region — the windows arms are SEMANTICALLY identical (same NT
calls, same flag bits, same control flow); 0.16 only reworks syntax (packed
flag structs like `.{ .COMMIT = true, .RESERVE = true }`, a hoisted
`current_process` local, `aligned_len` renamed `page_aligned_len`). Port from
the 0.15.2 text; the 0.16 diff is cosmetic.

Rules: PORT_RULES **R1** (fidelity — port the oracle's windows arms, don't
invent), **R3** (bind the SAME system libraries — `ntdll`/`kernel32`/`ucrt` via
hand-rolled `extern "system"`/`extern "C"`, the established `windows.rs`
pattern; **no `windows-sys` dependency is added**, see WI-0), **R4** (no new
crate edges — `rustty-vt` already depends on `rustty-os`, exactly the edge the
oracle's `page.zig → os/windows.zig` import maps to). Memory:
`rustty-completion-plan` (this un-defers part of GAP_ANALYSIS workstream 6 per
the user's directive), `derived-gate-green-can-be-bookkeeping` (every gate
below was actually RUN, see §5).

---

## 0. Verdict up front — the design is prototyped end-to-end and observed green

Every work item below was applied to a scratch shadow copy of the tree
(`$SCRATCHPAD/shadow`, rsync of main @ `d34cc154`) and the end-state gates were
**observed**:

| Gate | Result |
|---|---|
| `RUSTTY_SYS_CHECK_ONLY=1 cargo check -p rustty-os -p rustty-vt -p rustty-vt-c --target x86_64-pc-windows-msvc` | **green** (one pre-existing warning, see WI-5) |
| `cargo check -p rustty-os -p rustty-vt -p rustty-vt-c` (host) | **green, exit 0** |
| `cargo test -p rustty-os --lib` (host) | **70 passed, 0 failed** (69 pre-existing + 1 new) |
| `cargo check -p rustty-os --target x86_64-unknown-freebsd` | 1 error **before AND after** — pre-existing `pty.c` cc failure, NOT this finding, not regressed (see §6.6) |

Three discoveries that materially shaped the design (vs the F-001 vet text):

1. **The oracle has its own `AllocWindows`.** `ghostty src/terminal/page.zig:34-38`
   selects `AllocWindows` (`page.zig:58-83`): `VirtualAlloc(MEM_COMMIT |
   MEM_RESERVE, PAGE_READWRITE)` / `VirtualFree(0, MEM_RELEASE)` reached
   through `os/windows.zig`'s `exp.kernel32`. rustty-vt's `page.rs` port
   currently has **no windows arm at all** (`MAP_PRIVATE`/`MAP_ANONYMOUS`
   consts exist only for macos/linux), so `cargo check -p rustty-vt` for
   windows would fail even after rustty-os is fixed. WI-8 ports it.
2. **`std.heap.page_allocator`'s windows arm is ntdll, not VirtualAlloc.** Zig
   0.15.2/0.16 `std.heap.PageAllocator.map` uses
   `ntdll.NtAllocateVirtualMemory`/`NtFreeVirtualMemory` with
   `MEM_RESERVE_PLACEHOLDER`/`MEM_PRESERVE_PLACEHOLDER` for over-aligned
   requests (`zstd:std/heap/PageAllocator.zig:29-88`). WI-6 ports that
   verbatim (not a VirtualAlloc approximation).
3. **The real blocker after rustty-os is the `-sys` build scripts.** With
   rustty-os fixed, `cargo check -p rustty-vt --target …-msvc` runs the
   `highway-sys`/`simdutf-sys`/`zlib-sys` build scripts, whose `cc` invocations
   need an MSVC sysroot this host does not have. (This was invisible in the
   vet's repro: cargo aborted at rustty-os first.) `cargo check` never links,
   so WI-9 adds an explicit opt-in check-only skip.

Error inventory (all 25 reproduced errors → work item): 6× E0433 `libc` in
`locale.rs` → WI-3; 12× E0433 `libc` in `mach.rs` → WI-1; 2× E0433 `libc` in
`path.rs` → WI-2; 2× `compile_error!` + 1× E0308 in `page_allocator.rs` →
WI-6; 1× `compile_error!` in `passwd.rs` → WI-4; 1× E0277 in `pty.rs` → WI-7.
(Adversary re-verified this census independently on the LIVE tree, main @
`d34cc15`: exactly 25 errors, same per-file split.)

---

## WI-0 — Dependency & cfg strategy (no Cargo.toml changes)

**Files:** none (decision record).

- `crates/rustty-os/Cargo.toml:70-71` keeps `libc` gated
  `[target.'cfg(target_family = "unix")'.dependencies]`. That gate is CORRECT;
  the bug is windows-visible *uses* of `libc`, fixed per-module below.
- **No `windows-sys` crate.** The repo's established pattern is hand-rolled
  `extern "system"` bindings in `crates/rustty-os/src/windows.rs` (the R3 image
  of the oracle's `os/windows.zig`, which itself declares
  `pub extern "kernel32" fn …` — `ghostty src/os/windows.zig:112-253`). All new
  API surface (WI-5, WI-6) follows it. Adding `windows-sys` would be a new
  workspace dependency for ~10 symbols the oracle hand-declares — rejected.
- cfg vocabulary used below (matches the crate's existing usage):
  `windows` = `target_os = "windows"`; `unix` = `target_family = "unix"`.
- The **Zig-laziness doctrine** already recorded at
  `crates/rustty-os/src/lib.rs:28-37` (§3.4): Zig analyzes decls lazily, Rust
  compiles eagerly, so decls the oracle never analyzes on windows are `cfg`'d
  out; decls the oracle DOES reach on windows must compile. Per-module calls:
  - windows-REACHABLE in the oracle (must compile): `pty.zig` WindowsPty,
    `page.zig` AllocWindows, `std.heap.page_allocator`, `locale.zig`
    `ensureLocale` (windows arm at `ghostty src/os/locale.zig:35-46`),
    `path.zig` `expand` (windows runtime surface: `isExecutable`'s
    `.windows => true` at `ghostty src/os/path.zig:67` and the windows `';'`
    `std.fs.path.delimiter`; `:81`'s `hostname.exe` is TEST code, outside the
    lib check), `os/windows.zig` itself, homedir/hostname (already have
    windows arms).
  - windows-UNREACHED in the oracle (cfg out): `mach.zig` (darwin-only
    references: `ghostty src/os/main.zig:24` import is lazy, `:66-69` test
    reference is inside the `isDarwin()` arm), `passwd.get` (`@compileError`,
    `ghostty src/os/passwd.zig:36`), `locale.zig`'s
    `LC_ALL_MASK`/`locale_t`/`newlocale`/`freelocale` lazy aliases
    (`ghostty src/os/locale.zig:225-226,228-229` — never referenced on
    windows; `setlocale` at `:227` and `LC_ALL` at `:224` ARE
    windows-analyzed, hence WI-3's split).

---

## WI-1 — `lib.rs`: gate `mach` to unix

**File:** `crates/rustty-os/src/lib.rs` (module list, currently line ~78-79).

`mach.rs` is 12 of the 25 errors (raw `libc::mmap`/`munmap`/`sysconf`). The
oracle only analyzes `os/mach.zig` from darwin code
(`src/terminal/PageList.zig:529-530` — inside the `isDarwin()` branch of
`pageAllocator()`, `:518-530`; `os/main.zig:66-69`'s darwin-only test arm);
it is never analyzed on windows.

**Edit:**

```rust
-#[cfg(not(target_arch = "wasm32"))]
+#[cfg(target_family = "unix")]
 pub mod mach;
```

(wasm32 is not `unix`, so the old exclusion is subsumed.) Extend the module's
adjacent PORT NOTE: mach is posix-`mmap`-bound; the windows analog of Zig's
lazy non-analysis is the unix gate (§3.4 doctrine). `page_allocator.rs`'s
`use crate::mach::…` imports are already darwin-gated (lines 50-54) — no edit.

**Check:** windows error count for `mach.rs` drops 12 → 0
(`cargo check -p rustty-os --target x86_64-pc-windows-msvc` still red until WI-7).
Host: `cargo check -p rustty-os` stays green (macOS/linux/freebsd are all unix).

---

## WI-2 — `path.rs`: `std.fs.max_path_bytes` shim

**File:** `crates/rustty-os/src/path.rs` (line 53 + shim block at ~119).

Oracle: `ghostty src/os/path.zig:23` `var path_buf: [std.fs.max_path_bytes]u8`.
`std.fs.max_path_bytes` = `posix.PATH_MAX` on unix, and on windows
`PATH_MAX_WIDE * 3 + 1` (WTF-8 worst case) = `32767 * 3 + 1` = **98302**
(`zstd:std/fs.zig:52-63`, `zstd:std/os/windows.zig:4096`).

**Edit:** in the "support shims" section (before `PATH_SEP`, ~line 119):

```rust
// `std.fs.max_path_bytes`.
#[cfg(unix)]
const MAX_PATH_BYTES: usize = libc::PATH_MAX as usize;
#[cfg(windows)]
const MAX_PATH_BYTES: usize = 32767 * 3 + 1;
```

and at line 53:

```rust
-    let mut path_buf: [u8; libc::PATH_MAX as usize] = [0; libc::PATH_MAX as usize];
+    let mut path_buf: [u8; MAX_PATH_BYTES] = [0; MAX_PATH_BYTES];
```

(Note: 98302 bytes on the stack on windows is exactly what the oracle does.)
The file's other windows arms (`PATH_SEP`, `isExecutable`, `tokenize_path`
`';'`, `os_str_from_bytes`) already exist and compile — no edits.

**Test (WI-10):** add to the existing `mod tests`:

```rust
/// The `std.fs.max_path_bytes` shim: on unix it IS the platform PATH_MAX; on
/// windows it is Zig's WTF-8 worst case `PATH_MAX_WIDE * 3 + 1` (98302).
/// Compiled per-target: the unix arm is proven on the macOS host + rvm, the
/// windows arm by any future windows test run.
#[test]
fn max_path_bytes_matches_reference() {
    #[cfg(unix)]
    assert_eq!(MAX_PATH_BYTES, libc::PATH_MAX as usize);
    #[cfg(windows)]
    assert_eq!(MAX_PATH_BYTES, 98302);
}
```

**Check:** `cargo test -p rustty-os --lib path` on host (observed green).

---

## WI-3 — `locale.rs`: cfg-split the `locale.h` aliases

**File:** `crates/rustty-os/src/locale.rs` (lines 417-454).

Oracle: `ghostty src/os/locale.zig` **is windows-reachable** (windows arm at
:35-46 — setlocale-from-env then a logged TODO fallback; `global.zig` calls
`ensureLocale`, and ghostty's windows-gnu lib CI builds it). Its C aliases
(:224-229) come from a `locale.h` import; on windows Zig lazily analyzes ONLY
`setlocale` + `LC_ALL` (the decls `ensureLocale` names). rustty's `locale.rs`
already ports the windows control-flow arms (lines 98-217) — only the six
`libc::` aliases break the build.

**Edits (validated verbatim in the shadow):**

```rust
+#[cfg(unix)]
 const LC_ALL: std::ffi::c_int = libc::LC_ALL;
+// `LC_ALL` from the ucrt's <locale.h> (value 0 on every Windows CRT).
+#[cfg(windows)]
+const LC_ALL: std::ffi::c_int = 0;
+#[cfg(unix)]
 #[allow(dead_code)]
 const LC_ALL_MASK: std::ffi::c_int = libc::LC_ALL_MASK;
 // PARITY: the reference type alias is `locale_t` (snake_case, from the C lib).
+#[cfg(unix)]
 #[allow(dead_code, non_camel_case_types)]
 type locale_t = libc::locale_t;
+
+// R3: bind the SAME CRT entry point the oracle's `c.setlocale` resolves to on
+// windows (ucrtbase `setlocale`). Hand-rolled extern per the windows.rs pattern.
+#[cfg(windows)]
+mod c_locale {
+    extern "C" {
+        pub fn setlocale(
+            category: std::ffi::c_int,
+            locale: *const std::ffi::c_char,
+        ) -> *mut std::ffi::c_char;
+    }
+}
```

In `fn setlocale` (line ~434), split the one call site:

```rust
+    #[cfg(unix)]
     let p = unsafe { libc::setlocale(category, locale.as_ptr()) };
+    #[cfg(windows)]
+    let p = unsafe { c_locale::setlocale(category, locale.as_ptr()) };
```

Gate `unsafe fn newlocale` and `unsafe fn freelocale` (lines 443-454) with
`#[cfg(unix)]` (Zig-lazy: never analyzed on windows; they are POSIX-only
symbols the windows CRT does not export). Add a PORT NOTE citing
`ghostty src/os/locale.zig:224-229` (the alias block; on windows only `LC_ALL`
`:224` + `setlocale` `:227` are analyzed) and this doc.

**Check:** locale error count 6 → 0 on the windows target; host tests
(`locale` negative-control test) stay green (observed).

---

## WI-4 — `passwd.rs`: the lazy-`@compileError` equivalent

**Files:** `crates/rustty-os/src/passwd.rs` (lines 182-188, 14),
`crates/rustty-os/src/homedir.rs` (lines 13-14).

Oracle: `ghostty src/os/passwd.zig:36`
`if (builtin.os.tag == .windows) @compileError("passwd is not available on windows");`
— **lazy**: it fires only if `get` is *analyzed from a windows-reachable call
site*. Ghostty's windows builds pass because none exists (`passwd.zig:20-24`
even gates the `pwd.h` import off windows; the file's test self-skips,
`passwd.zig:148`). Rust's `compile_error!` is eager — the current port breaks
every windows build unconditionally, which is the semantic delta the vet
flagged.

**Chosen mapping — the item is ABSENT on windows.** Zig's semantics are
"referencing `get` on windows is a compile-time error; not referencing it
compiles". The Rust construct with exactly those semantics is cfg-ing the item
out: a windows call site fails at the CALL SITE (E0425) at compile time, and a
windows build with no call site compiles. A runtime `unreachable!()` body was
considered and REJECTED: it would make a windows call site *compile* — strictly
weaker than the oracle's guarantee. (Precedent: the repo already maps
windows-unreachable `@compileError` decls to absence — e.g.
`rustty-xev/src/iocp_watcher.rs:731` "has NO Rust image".)

**Edits (validated):** replace lines 182-188 with a comment only:

```rust
// PORT NOTE: `if (builtin.os.tag == .windows) @compileError("passwd is not
// available on windows");` — lazy in Zig: it fires only when `get` is analyzed
// from a windows-reachable call site (ghostty windows builds pass because none
// exists). The Rust analog with the same semantics is the ABSENCE of the item
// on windows: any windows call site is a compile error at the CALL SITE, and a
// windows build with no call site compiles — exactly the reference's behavior.
```

(The unix `get` at lines 51-52 already carries `#[cfg(not(windows))]`; `Entry`/
`Error` stay windows-visible, matching the oracle's windows-visible struct.)
Plus two warning fixes the change surfaces:

- `passwd.rs:14` — gate the now-windows-unused import:
  `#[cfg(not(windows))] use rustty_build_config as build_config;`
  (its only use, `:83`, is inside flatpak/non-windows code).
- `homedir.rs:13-14` — the import is ALREADY gated
  `#[cfg(not(target_os = "macos"))]`; EXTEND that gate, do not replace it:
  `#[cfg(not(any(target_os = "macos", windows)))] use crate::passwd;`.
  (A bare `#[cfg(not(windows))]` would re-enable the import on macOS — where
  `home()`'s passwd block `homedir.rs:112-114` is cfg'd out — and regress the
  primary host lane with an unused-import warning. homedir's windows arm
  `:146` reads `USERPROFILE`/`GetTempPathW` and never touches passwd.)

**Check:** the `passwd` compile_error disappears from the windows lane; host
`cargo test -p rustty-os --lib passwd` green (observed).

---

## WI-5 — `windows.rs`: the VM constants + `exp.kernel32.VirtualAlloc/VirtualFree`

**File:** `crates/rustty-os/src/windows.rs`.

Oracle: `ghostty src/os/windows.zig:84-88` top-level
`MEM_COMMIT = 0x1000; MEM_RELEASE = 0x8000; MEM_RESERVE = 0x2000;
PAGE_READWRITE = 0x04;` and `:205-216` `pub extern "kernel32" fn VirtualAlloc /
VirtualFree` inside `exp`'s kernel32 (`:112-113`). The module's §E25 PORT NOTE
(lines 15-22) currently says these were "deliberately not transcribed: dead
surface" — **that rationale is now obsolete** because WI-8's `page.rs`
AllocWindows is their live caller (same caller as the oracle's,
`page.zig:62,76`). Revise that PORT NOTE accordingly.

**Edits (validated):** in the top-level constants block (after `MAX_PATH`,
line ~190):

```rust
pub const MEM_COMMIT: DWORD = 0x1000;
pub const MEM_RELEASE: DWORD = 0x8000;
pub const MEM_RESERVE: DWORD = 0x2000;
pub const PAGE_READWRITE: DWORD = 0x04;
```

and in `exp::kernel32`'s `extern "system"` block (after `CancelIoEx`, ~line 288):

```rust
pub fn VirtualAlloc(
    lpAddress: super::super::LPVOID,
    dwSize: SIZE_T,
    flAllocationType: DWORD,
    flProtect: DWORD,
) -> super::super::LPVOID;
pub fn VirtualFree(
    lpAddress: super::super::LPVOID,
    dwSize: SIZE_T,
    dwFreeType: DWORD,
) -> BOOL;
```

**Optional polish (pre-existing, NOT introduced here):** the windows lane's one
surviving warning is `windows.rs:210` `use super::{c_void, BOOL, DWORD,
DWORD_PTR, HANDLE, LPWSTR, PVOID, SIZE_T};` (unused at `exp`'s top level —
`exp::kernel32` has its own imports). Prune to the names `exp`'s own items use.

**Check:** `cargo check -p rustty-os --target x86_64-pc-windows-msvc` compiles
this module (it is `#![cfg(windows)]`, so ONLY the windows lane sees it — the
host lane cannot regress).

---

## WI-6 — `page_allocator.rs`: port the Zig-std windows mapping arm

**File:** `crates/rustty-os/src/page_allocator.rs` (replace lines 350-367).

Oracle chain: `ghostty src/terminal/PageList.zig:518-529` (`pageAllocator()`
returns `std.heap.page_allocator` on non-darwin, :525) →
`zstd:std/heap/PageAllocator.zig` windows arms: `map` :29-88, `unmap` :145-149,
`realloc` :160-178. Constants: `zstd:std/os/windows.zig:3595-3626`
(`MEM_COMMIT 0x1000`, `MEM_RESERVE 0x2000`, `MEM_RELEASE 0x8000`,
`MEM_RESERVE_PLACEHOLDER 0x00040000`, `MEM_PRESERVE_PLACEHOLDER 0x00000400`,
`PAGE_NOACCESS 0x01`, `PAGE_READWRITE 0x04`). Externs:
`zstd:std/os/windows/ntdll.zig:363-380`. Pseudo-handle:
`zstd:std/os/windows.zig:158-161` (`GetCurrentProcess()` is inline
`@bitCast(@as(isize, -1))`, NOT a kernel32 call — port it as a const fn).
`std.heap.pageSize()` on windows folds to the comptime constant `4 << 10`
(`zstd:std/heap.zig:63-66`: `if (page_size_min == page_size_max) return
page_size_min;` — and `:702-712`/`:850+` give `4 << 10` for every windows
arch in both the min and max tables).

**Layering decision:** the ntdll externs/constants go in a **private
`win_ntdll` module inside `page_allocator.rs`**, NOT `windows.rs`. They are
vendored *Zig-std* surface; `windows.rs` mirrors the oracle's `os/windows.zig`
1:1, which has no ntdll bindings (its own §E25 note says so). Precedent:
`mach.rs`, `mem.rs`, and `page.rs` each declare their own private syscall
bindings.

**Edits (validated verbatim in the shadow — this exact code compiled):**

1. Replace the two `compile_error!` methods and their `#[cfg(all(not(unix),
   not(target_arch = "wasm32")))]` impl block (lines 350-367) with:

```rust
// Windows path: faithful port of the windows branch of Zig 0.15.2
// `std.heap.PageAllocator.map`/`unmap` (the same vendored-std oracle as the
// posix branch above): `ntdll.NtAllocateVirtualMemory` /
// `ntdll.NtFreeVirtualMemory` with `MEM_COMMIT | MEM_RESERVE` +
// `PAGE_READWRITE`, falling back to a `MEM_RESERVE_PLACEHOLDER` reservation
// that is split (`MEM_RELEASE | MEM_PRESERVE_PLACEHOLDER`) and re-committed at
// the aligned address when the requested alignment exceeds what the first
// mapping happened to give. Memory is zeroed on handout by the OS, exactly
// like the posix `mmap(MAP_ANON)` path.
//
// The ntdll externs live in `win_ntdll` below (NOT `crate::windows`): they are
// vendored Zig-STD surface, not part of the reference's `os/windows.zig`
// (whose port `windows.rs` mirrors 1:1), matching how `mach.rs`/`mem.rs`/
// `page.rs` declare their own private syscall bindings.
#[cfg(all(not(miri), windows))]
impl PageAllocator {
    fn map_pages(&self, n: usize, align: usize) -> Option<NonNull<u8>> {
        use win_ntdll::*;

        let page_size = page_size();
        if n >= usize::MAX - page_size {
            return None;
        }
        let alignment_bytes = align;

        let mut base_addr: PVOID = core::ptr::null_mut();
        let mut size: SIZE_T = n;

        let mut status = unsafe {
            NtAllocateVirtualMemory(
                GetCurrentProcess(),
                &mut base_addr,
                0,
                &mut size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };

        if status == SUCCESS && mem_is_aligned(base_addr as usize, alignment_bytes) {
            return NonNull::new(base_addr.cast());
        }

        if status == SUCCESS {
            let mut region_size: SIZE_T = 0;
            unsafe {
                NtFreeVirtualMemory(
                    GetCurrentProcess(),
                    &mut base_addr,
                    &mut region_size,
                    MEM_RELEASE,
                )
            };
        }

        // PORT NOTE: verbatim from the oracle (`n + alignment_bytes -
        // page_size`, zstd:std/heap/PageAllocator.zig:44). On the
        // aligned-MISS path this cannot underflow (the first mapping is
        // 64K-aligned, so a miss implies `alignment_bytes > 64K > page_size`).
        // On the first-alloc-FAILURE (OOM) path with `alignment_bytes <
        // page_size` and small `n` the oracle's own expression underflows
        // (Zig safe modes panic identically) — inherited as-is per R1, do not
        // "fix".
        let overalloc_len = n + alignment_bytes - page_size;
        let aligned_len = mem_align_forward(n, page_size);

        base_addr = core::ptr::null_mut();
        size = overalloc_len;

        status = unsafe {
            NtAllocateVirtualMemory(
                GetCurrentProcess(),
                &mut base_addr,
                0,
                &mut size,
                MEM_RESERVE | MEM_RESERVE_PLACEHOLDER,
                PAGE_NOACCESS,
            )
        };

        if status != SUCCESS {
            return None;
        }

        let placeholder_addr = base_addr as usize;
        let aligned_addr = mem_align_forward(placeholder_addr, alignment_bytes);
        let prefix_size = aligned_addr - placeholder_addr;

        if prefix_size > 0 {
            let mut prefix_base = base_addr;
            let mut prefix_size_param: SIZE_T = prefix_size;
            unsafe {
                NtFreeVirtualMemory(
                    GetCurrentProcess(),
                    &mut prefix_base,
                    &mut prefix_size_param,
                    MEM_RELEASE | MEM_PRESERVE_PLACEHOLDER,
                )
            };
        }

        let suffix_start = aligned_addr + aligned_len;
        let suffix_size = (placeholder_addr + overalloc_len) - suffix_start;
        if suffix_size > 0 {
            let mut suffix_base = suffix_start as PVOID;
            let mut suffix_size_param: SIZE_T = suffix_size;
            unsafe {
                NtFreeVirtualMemory(
                    GetCurrentProcess(),
                    &mut suffix_base,
                    &mut suffix_size_param,
                    MEM_RELEASE | MEM_PRESERVE_PLACEHOLDER,
                )
            };
        }

        base_addr = aligned_addr as PVOID;
        size = aligned_len;

        status = unsafe {
            NtAllocateVirtualMemory(
                GetCurrentProcess(),
                &mut base_addr,
                0,
                &mut size,
                MEM_COMMIT | MEM_PRESERVE_PLACEHOLDER,
                PAGE_READWRITE,
            )
        };

        if status == SUCCESS {
            return NonNull::new(base_addr.cast());
        }

        base_addr = aligned_addr as PVOID;
        size = aligned_len;
        unsafe {
            NtFreeVirtualMemory(GetCurrentProcess(), &mut base_addr, &mut size, MEM_RELEASE)
        };

        None
    }

    fn unmap_pages(&self, memory: &mut [u8]) {
        use win_ntdll::*;
        let mut base_addr: PVOID = memory.as_mut_ptr().cast();
        let mut region_size: SIZE_T = 0;
        unsafe {
            NtFreeVirtualMemory(
                GetCurrentProcess(),
                &mut base_addr,
                &mut region_size,
                MEM_RELEASE,
            )
        };
    }
}

// Vendored Zig-std windows VM surface (Zig 0.15.2 `std.os.windows`): the exact
// constants/externs the `std.heap.PageAllocator` windows branch names. Private
// to this module (see the layering note above).
#[cfg(all(not(miri), windows))]
mod win_ntdll {
    pub type PVOID = *mut core::ffi::c_void;
    pub type SIZE_T = usize;
    pub type HANDLE = PVOID;
    pub type NTSTATUS = i32;
    /// `NTSTATUS.SUCCESS`
    pub const SUCCESS: NTSTATUS = 0;

    pub const MEM_COMMIT: u32 = 0x1000;
    pub const MEM_RESERVE: u32 = 0x2000;
    pub const MEM_RELEASE: u32 = 0x8000;
    pub const MEM_RESERVE_PLACEHOLDER: u32 = 0x00040000;
    pub const MEM_PRESERVE_PLACEHOLDER: u32 = 0x00000400;
    pub const PAGE_NOACCESS: u32 = 0x01;
    pub const PAGE_READWRITE: u32 = 0x04;

    #[link(name = "ntdll")]
    extern "system" {
        pub fn NtAllocateVirtualMemory(
            ProcessHandle: HANDLE,
            BaseAddress: *mut PVOID,
            ZeroBits: usize,
            RegionSize: *mut SIZE_T,
            AllocationType: u32,
            Protect: u32,
        ) -> NTSTATUS;
        pub fn NtFreeVirtualMemory(
            ProcessHandle: HANDLE,
            BaseAddress: *mut PVOID,
            RegionSize: *mut SIZE_T,
            FreeType: u32,
        ) -> NTSTATUS;
    }

    /// `windows.GetCurrentProcess()` — the process pseudo-handle; the Zig
    /// oracle inlines `@bitCast(@as(isize, -1))` (std/os/windows.zig:158-161)
    /// rather than calling kernel32.
    #[allow(non_snake_case)]
    pub fn GetCurrentProcess() -> HANDLE {
        usize::MAX as HANDLE
    }
}
```

2. Helper cfg extensions (same file, lines 330-348):
   - `page_size()`: add a windows variant returning `4 << 10` with the
     `std.heap.pageSize()` comptime-fold PORT NOTE (cite above).
   - `mem_align_forward`: widen its cfg to
     `#[cfg(any(windows, all(unix, not(any(target_os = "macos", target_os = "ios")))))]`.
   - add `#[cfg(all(not(miri), windows))] fn mem_is_aligned(addr: usize,
     alignment: usize) -> bool { addr & (alignment - 1) == 0 }`
     (`mem.isAligned`; alignment is a power of two by the Allocator contract).
   - `align_pointer` stays unix-only (the windows arm never calls it).

3. **`ResizeInPlace` is deliberately NOT implemented for windows** — identical
   to the existing generic-unix decision recorded at lines 219-224 ("only the
   Darwin path is wired per the macOS-first plan; the arena's manual-realloc
   fallback remains correct"). The Zig windows `realloc` shrink
   (`MEM_RESET` decommit, `zstd:std/heap/PageAllocator.zig:160-178`) is a
   PERF-only follow-up; note it in the impl's PORT NOTE so the deferral is
   explicit, not silent.

This also dissolves the E0308 at old line 360 (the `()` body).

**Test (WI-10):** the KAT below, in the existing `#[cfg(all(test, not(miri)))]
mod tests` (validated compiling + passing on host, running on rvm):

```rust
/// KATs for the vendored-std alignment helpers shared by the generic-unix and
/// windows mapping arms (`mem.alignForward` / `mem.isAligned`). Runs on every
/// target that compiles those arms (Linux rvm lane, a future windows host);
/// Darwin uses `mach.rs`'s own copies and skips this.
#[cfg(any(windows, all(unix, not(any(target_os = "macos", target_os = "ios")))))]
#[test]
fn align_helpers_match_reference() {
    assert_eq!(mem_align_forward(0, 4096), 0);
    assert_eq!(mem_align_forward(1, 4096), 4096);
    assert_eq!(mem_align_forward(4096, 4096), 4096);
    assert_eq!(mem_align_forward(4097, 4096), 8192);
    #[cfg(windows)]
    {
        assert!(mem_is_aligned(0, 4096));
        assert!(mem_is_aligned(8192, 4096));
        assert!(!mem_is_aligned(4097, 4096));
    }
}
```

**Check:** `cargo check -p rustty-os --target x86_64-pc-windows-msvc` → after
WI-1..WI-7 this is **green with zero rustty-os errors** (observed).

---

## WI-7 — `pty.rs`: `WindowsPtyError` gets `Display + Error`

**File:** `crates/rustty-os/src/pty.rs` (after the enum at lines 528-533).

Oracle: `ghostty src/pty.zig:341` `OpenError = error{Unexpected}`, `:472`
`SetSizeError = error{ResizeFailed}`, `:450` `if (result != windows.S_OK)
return error.Unexpected;`. The port's `open` returns
`Result<Pty, Box<dyn std::error::Error>>` (it must carry both `io::Error` from
`unexpectedError` and `WindowsPtyError`), so the enum needs the std `Error`
trait — the E0277 at line 649. Zig error values format as their names.

**Edit (validated):**

```rust
// PORT NOTE: `OpenError = error{Unexpected}` / `SetSizeError = error{ResizeFailed}`.
// Zig error values format as their names; `open` returns them through the
// `Box<dyn std::error::Error>` seam, which requires the std Error trait.
#[cfg(windows)]
impl std::fmt::Display for WindowsPtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WindowsPtyError::Unexpected => write!(f, "Unexpected"),
            WindowsPtyError::ResizeFailed => write!(f, "ResizeFailed"),
        }
    }
}

#[cfg(windows)]
impl std::error::Error for WindowsPtyError {}
```

No other WindowsPty edit is needed: the struct/`open`/`deinit`/`getSize`/
`setSize`/`getForegroundPid`/`getTtyName` bodies (lines 509-703) were verified
line-by-line against `ghostty src/pty.zig:326-495` — they already match
(pipe-name prefix `rustty-pty` is the R2 identity retarget) and, with WI-1..6
in place, they type-check (observed).

**Check:** rustty-os windows lane green:
`cargo check -p rustty-os --target x86_64-pc-windows-msvc` (observed — this is
the first all-green rustty-os point; commit here).

---

## WI-8 — `rustty-vt/src/page.rs`: port `AllocWindows`

**File:** `crates/rustty-vt/src/page.rs` (mod `page_alloc`, lines 228-360).

Oracle: `ghostty src/terminal/page.zig:34-38` (comptime switch), `:58-83`
(`AllocWindows` — `VirtualAlloc(null, n, MEM_COMMIT | MEM_RESERVE,
PAGE_READWRITE) orelse error.OutOfMemory`; `VirtualFree(ptr, 0, MEM_RELEASE)`),
reached as `windows.exp.kernel32.VirtualAlloc` — i.e. **through
`os/windows.zig`**, which maps to `rustty_os::windows` (edge already exists,
`crates/rustty-vt/Cargo.toml:103`; R4-clean). The current port has only
`AllocPosix` + a wasm arm; its `MAP_PRIVATE`/`MAP_ANONYMOUS` consts exist only
for macos/linux, so the windows target dies with E0425s here once rustty-os
compiles.

**⚠ w1 COLLISION (verified in `lanes-w1.json`):** wave-1 lane `w1-L01` edits
THIS FILE — batch `b10`'s **W60-B01-001** targets exactly this `page_alloc`
cfg region (its finding: the posix items are compiled under broad non-wasm
cfgs while `MAP_PRIVATE`/`MAP_ANONYMOUS` exist only for macos/linux — i.e. it
will restructure the very cfg lines step 1 below rewrites; `b10` also carries
W60-PAGE-ALIGN-01/W61-B05-001 and `b11` W62-B04-001/W62-B06-001 elsewhere in
page.rs). **Apply WI-8 only after w1-L01 lands**, re-read the then-current
`page_alloc`, and apply the edits below as PATTERNS (windows must be excluded
from whatever posix cfg w1 settled on; the windows arm added alongside), not
as verbatim line edits. The line numbers in this WI are from the pre-w1 tree.

**Edits (validated on the pre-w1 tree):** inside `mod page_alloc`:

1. Exclude windows from the posix items: on `pub fn alloc` / `pub fn free`
   (lines ~253/~276) and on the `PROT_READ`/`PROT_WRITE`/`MAP_FAILED` consts
   and the `extern "C"` mmap/munmap block, change
   `#[cfg(not(target_arch = "wasm32"))]` →
   `#[cfg(not(any(target_arch = "wasm32", windows)))]`.
   (`MAP_PRIVATE`/`MAP_ANONYMOUS` stay macos/linux — unchanged.)
2. Add the windows arm (replaces the "Windows variant's comments are preserved
   here for fidelity" placeholder note at lines 241-243 with live code):

```rust
// `AllocWindows` — allocate page-aligned, zeroed backing memory using
// VirtualAlloc with MEM_COMMIT | MEM_RESERVE which guarantees zeroed pages
// (reference `src/terminal/page.zig` AllocWindows; bindings from
// `os/windows.zig` exactly as the reference reaches
// `windows.exp.kernel32.VirtualAlloc`).
/// pub fn alloc(n: usize) error{OutOfMemory}![]align(page_size_min) u8
#[cfg(windows)]
pub fn alloc(n: usize) -> Option<Backing> {
    use rustty_os::windows;
    let addr = unsafe {
        windows::exp::kernel32::VirtualAlloc(
            core::ptr::null_mut(),
            n,
            windows::MEM_COMMIT | windows::MEM_RESERVE,
            windows::PAGE_READWRITE,
        )
    };
    if addr.is_null() {
        return None; // orelse return error.OutOfMemory
    }
    Some(Backing { ptr: addr as *mut u8, len: n })
}

/// pub fn free(mem: []align(page_size_min) u8) void
#[cfg(windows)]
pub fn free(b: &Backing) {
    use rustty_os::windows;
    unsafe {
        windows::exp::kernel32::VirtualFree(
            b.ptr as *mut core::ffi::c_void,
            0,
            windows::MEM_RELEASE,
        );
    }
}
```

`PAGE_SIZE_MIN` (page.rs:200-226) already falls back to `4 << 10` on windows —
matches `zstd:std/heap.zig:702+`; no edit. The rest of rustty-vt was audited
for windows-hostility: `mem.rs` (madvise) already stubs non-{mac,ios,linux};
`kitty/graphics_image.rs` already has full windows arms (shm → 
`UnsupportedMedium`, `realpath`/`open_file`/`unlink` windows variants);
`pagelist/bench.rs`'s `libc::getrusage` is dev/test-scoped and outside the lib
check. **rustty-vt-c needed zero edits at design time** (no OS surface;
verified by the green gate) — but w1 lane `w1-L02-b03` (W36-B02) is editing
`crates/rustty-vt-c/build.rs`/`src/build_info.rs`; re-run the windows gate
after it lands, and if that build.rs grows a `cc` step, give it the WI-9
guard too.

**Check:** with WI-9's env set, `cargo check -p rustty-vt -p rustty-vt-c
--target x86_64-pc-windows-msvc` green (observed); host
`cargo check -p rustty-vt` and `cargo test -p rustty-vt` unaffected
(windows-cfg'd code only).

---

## WI-9 — the `-sys` check-only cross lane (`RUSTTY_SYS_CHECK_ONLY`)

**Files:** `crates-sys/highway-sys/build.rs`, `crates-sys/simdutf-sys/build.rs`,
`crates-sys/zlib-sys/build.rs`, `crates/rustty-simd/build.rs` (its own cc build
consumes `DEP_HWY_INCLUDE`).

With rustty-os green, the windows lane's next failure is these build scripts:
`cc` cross-compiling vendored C/C++ to `x86_64-pc-windows-msvc` needs MSVC
headers this host does not have (`fatal error: 'stdlib.h' file not found` —
observed). `cargo check` **never links**, so the native objects are pure waste
in a check lane. Precedent for target-conditional skips already in-tree:
`crates/rustty-os/build.rs:23-25` skips `pty.c` off {macos,linux,freebsd}.

**Edit (identical first-lines-of-`fn main()` block in all four, validated):**

```rust
    // Check-only cross lane (W2D_WINDOWS_OS / F-001): type-checking a target
    // this host has no C toolchain/sysroot for (e.g. `cargo check --target
    // x86_64-pc-windows-msvc` on a macOS host). `cargo check` never links, so
    // the native objects are not needed; opt-in via env so a real build can
    // never silently skip.
    println!("cargo:rerun-if-env-changed=RUSTTY_SYS_CHECK_ONLY");
    if std::env::var_os("RUSTTY_SYS_CHECK_ONLY").is_some() {
        return;
    }
```

Design properties: (a) **opt-in** — unset, behavior is byte-identical, so
normal builds/tests cannot silently lose the native code (a build with it
wrongly set fails loudly at link time); (b) `rerun-if-env-changed` makes the
lane toggle sound under cargo fingerprinting; (c) the skip is
target-independent by construction, which keeps it honest — it is a
*check-lane* switch, not a windows special-case. `dcimgui-sys`/`wuffs-sys`/
`oniguruma-sys` are feature-gated OFF in this gate's graph and need no edit
(re-audit if the gate ever grows `--features`).

**Check:** the end-state gate (§5, observed green). Also observed: WITHOUT the
env, the same command still fails in the -sys build scripts — proving the
guard, not luck, is what closes the lane.

---

## WI-10 — the gate script

**File (new):** `.port/tools/gates/w2f001_windows_check_gate.sh` (pattern:
`ws19_portal_openuri_gate.sh` — bash, `set -euo pipefail`, loud PASS/FAIL).

```bash
#!/usr/bin/env bash
# W2/F-001 gate: the shared OS layer + VT + VT-C type-check for windows-msvc,
# and the host stays green. See .port/codex-vet/designs/W2D_WINDOWS_OS.md.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# 1. The windows lane (check-only: no linking, native -sys builds skipped).
RUSTTY_SYS_CHECK_ONLY=1 cargo check -p rustty-os -p rustty-vt -p rustty-vt-c \
  --target x86_64-pc-windows-msvc

# 2. Host must not regress (env UNSET: the real native path).
cargo check -p rustty-os -p rustty-vt -p rustty-vt-c
cargo test -p rustty-os --lib

echo "PASS: F-001 windows check gate"
```

**Check:** run it. Wire into `.port/tools/ci.sh` alongside the existing gates
(follow that file's conventions when adding).

---

## WI-11 — Linux (rvm) execution lane

The generic-unix `map_pages`/`unmap_pages` shares its helper math and its
Zig-std oracle file with the new windows arm, and it EXECUTES on the Fedora VM.
Run (established rsync pattern from `ws19_portal_openuri_gate.sh`):

```bash
rsync -a --delete --exclude target --exclude .git ./ rvm:~/rustty-vmwork/
ssh rvm 'cd ~/rustty-vmwork && cargo test -p rustty-os --lib && cargo test -p rustty-vt --lib pagelist'
```

This executes: the generic-unix mmap arm of `page_allocator.rs` (its 2
non-darwin tests + the new `align_helpers_match_reference` KAT),
`max_path_bytes_matches_reference` (unix arm), the locale/passwd/homedir
edits on a second libc, and PageList on the generic page allocator.

---

## 4. Porting order

1. **WI-1..WI-7** (rustty-os, one commit) → checkpoint:
   `cargo check -p rustty-os --target x86_64-pc-windows-msvc` green
   AND host `cargo test -p rustty-os --lib` green (70 tests).
2. **WI-8** (rustty-vt) + **WI-9** (-sys lane) — WI-8 needs WI-5's bindings
   AND must wait for w1-L01 to land in `crates/rustty-vt/src/page.rs` (merge
   by pattern, see WI-8's collision banner); WI-9 is independent but only
   observable with WI-8 done. One commit.
3. **WI-10** gate script + ci.sh wiring; run it.
4. **WI-11** rvm lane; record results in the ledger.

Iteration note for fix agents: the 25-error inventory (§0) was measured to be
COMPLETE for rustty-os (observed: zero errors after WI-1..7 with no further
iteration), and rustty-vt/vt-c green after WI-8/9. If the live tree has
drifted (w1 wave), re-run the failing check after each step and extend the
matching WI's pattern rather than inventing new mechanisms.

## 5. End-state gate (all observed green in the validated prototype)

```
RUSTTY_SYS_CHECK_ONLY=1 cargo check -p rustty-os -p rustty-vt -p rustty-vt-c --target x86_64-pc-windows-msvc
cargo check -p rustty-os -p rustty-vt -p rustty-vt-c        # host, env unset
cargo test  -p rustty-os --lib                              # host: 70 passed
# + WI-10 script, + WI-11 rvm lane
```

## 6. Open risks / non-goals

1. **The windows arms are type-checked, never executed** — no windows host or
   CI exists. Mitigations: WI-6/WI-8 are verbatim ports of known-good oracles
   (Zig std 0.15.2 == 0.16 windows arms; ghostty's CI-tested page.zig/pty.zig);
   every constant was read out of the Zig std source, not from memory. A real
   windows build/test lane (oracle: ghostty `test.yml` build-libghostty-vt-
   windows/test-windows) is the honest follow-up and out of this finding's
   scope.
2. **`cargo check` (lib-only) does not type-check `#[cfg(test)]`/dev-dep code
   for windows** (e.g. `pagelist/bench.rs`'s `getrusage`, windows-gated tests).
   A future `--tests` windows lane will surface more work; keep new
   windows-only test code minimal until then.
3. **Placeholder-API floor:** `MEM_RESERVE_PLACEHOLDER` re-commit requires
   Windows 10 1803+ — inherited from the Zig-std oracle (same floor), and only
   reached for over-page alignments; not a new constraint.
4. **`RUSTTY_SYS_CHECK_ONLY` misuse** fails loudly at link time, never
   silently; it must never be set in default build/test invocations, and the
   gate script is its only sanctioned user.
5. **w1 churn — NOT disjoint (adversary-corrected from `lanes-w1.json`):** w1
   spans 16 lanes / ~149 files and OVERLAPS this design in three places:
   (a) `crates/rustty-vt/src/page.rs` — `w1-L01-b10`/`b11`, incl.
   **W60-B01-001 in the same `page_alloc` cfg region WI-8 edits** (see the
   collision banner in WI-8: sequence WI-8 after w1-L01, merge by pattern);
   (b) `crates/rustty-vt-c/build.rs` — `w1-L02-b03` (W36-B02): re-run the
   gate after it lands (see WI-8's note); (c) `crates/rustty-os/src/xdg.rs`
   — `w1-L03-b02`: same crate but no WI touches xdg.rs; no action.
   All other files in this design's set (rustty-os modules, the three
   `crates-sys/*` build.rs of WI-9, rustty-simd/build.rs) appear in no w1
   lane. Fix agents must still re-verify per-file before editing.
6. **FreeBSD lane is NOT fixed here** (separate finding): its blocker is
   `rustty-os/build.rs` cc-compiling `pty.c` for a freebsd sysroot the host
   lacks — the WI-9 guard placed in `crates/rustty-os/build.rs`'s two
   `cc::Build` sections would close it the same way; flagged for that
   finding's designer.
7. **Windows `ResizeInPlace`** (Zig realloc `MEM_RESET` shrink) deliberately
   deferred with an explicit PORT NOTE (WI-6.3) — same standing decision as
   the generic-unix arm; PERF-only, correctness covered by the arena fallback.
8. **rustty-termio's windows lane is explicitly OUT of this design's scope.**
   The vet's cutoff chain names "vt, vt-c, and termio" as unreachable on
   windows, and the oracle's windows CI is broader than this gate
   (`test-windows` runs the FULL `zig build -Dapp-runtime=none test`, and
   `build-libghostty-windows-gnu` builds full libghostty —
   `ghostty .github/workflows/test.yml:582,1481-1495`). This design restores
   the surface matching **build-libghostty-vt-windows** (os + vt + vt-c,
   including the ConPTY `pty.rs` the finding names); the termio windows
   spawn/tty path remains the GAP_ANALYSIS workstream-6 deferral and needs
   its own finding/design — stated here so the drop is explicit, not silent.
