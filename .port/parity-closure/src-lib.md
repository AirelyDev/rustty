# Parity closure checklist — src/lib

15 open finding(s): 0 high, 3 medium, 12 low. Full evidence per row is in [`docs/parity-audit/src-lib.md`](../../docs/parity-audit/src-lib.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## Medium (3)

- [x] **S110-F1** · CONFIRMED · `ALLOC` · missing-error-path
  - **What:** GlobalZigAllocator::alloc panics (aborts across the C ABI) on an over-large len instead of returning null
  - **Oracle:** `src/lib/allocator.zig:235-236 (std.heap.c_allocator backend; malloc failure -> null -> OutOfMemory)` → **Rustty:** `crates/rustty-lib/src/allocator.rs:510-528`
  - **Audit's suggested fix:** Use `Layout::from_size_align(..).ok()` and return `null_mut()` (alloc/remap) or `false` (resize) on Err.
- [ ] **S111-F1** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** allocTmpDir treats an empty TMPDIR/TMP as unset (oracle uses the empty value)
  - **Oracle:** `src/os/file.zig:85-86` → **Rustty:** `crates/rustty-os/src/file.rs:138-147`
  - **Audit's suggested fix:** Drop the `.filter(|v| !v.is_empty())` in file.rs:141 and environ.rs:410 so an empty TMPDIR/TMP is returned (trimmed) exactly as the oracle does, or document/justify the deviation if it is intentional and update both copies together.
- [ ] **S111-F3** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** WaitError drops the xev.Timer.RunError half; a timer failure is reported to the callback as FlatpakSpawnFail
  - **Oracle:** `src/os/flatpak.zig:110,220-228,462-470` → **Rustty:** `crates/rustty-os/src/flatpak.rs:348-349,870-873`
  - **Audit's suggested fix:** Make `WaitError` an enum carrying both `Error` and `xev::TimerRunError` variants and forward the timer error unchanged in `xev_timer_callback`.

## Low (12)

- [x] **S110-F3** · CONFIRMED · `INDIV` · weakened-assertion
  - **What:** Test `c allocator` does not exercise `c_allocator()`; the public fn is untested
  - **Oracle:** `src/lib/allocator.zig:246-253` → **Rustty:** `crates/rustty-lib/src/allocator.rs:665-683`
  - **Audit's suggested fix:** `let c_alloc = c_allocator(); let alloc = c_alloc.zig();` then the same alloc/len==10/free assertions.
- [ ] **S110-F5** · DISPUTED · `ABI` · missing-branch
  - **What:** PackedTaggedUnionLayout::validate drops the arm type-identity and Owner-identity compile-time checks
  - **Oracle:** `src/lib/packed.zig:165-178 (and 68-73)` → **Rustty:** `crates/rustty-lib/src/packed.rs:272-291 (and 154-163)`
  - **Audit's suggested fix:** Carry a `source_bit_size`/owner id per arm (or per-field zig type label) in the descriptor and check equality in validate().
- [ ] **S111-F10** · DISPUTED · `TEST` · missing-test
  - **What:** test "_ returns msgid at comptime" has no direct counterpart
  - **Oracle:** `src/os/i18n.zig:205-210` → **Rustty:** `crates/rustty-os/src/i18n.rs:420-428`
  - **Audit's suggested fix:** Add a cross-platform test `underscore_returns_msgid` asserting `gettext_(c"Rustty") == c"Rustty"` when no catalog is bound (or with i18n disabled).
- [ ] **S111-F11** · CONFIRMED · `INDIV` · missing-error-path
  - **What:** Shell-fallback home lookup drops the 512-byte stdout/stderr limits
  - **Oracle:** `src/os/homedir.zig:62-67` → **Rustty:** `crates/rustty-os/src/homedir.rs:138-159`
  - **Audit's suggested fix:** Cap captured stdout/stderr at 512 bytes and treat overflow as an error, mirroring std.process.run's StdoutStreamTooLong.
- [ ] **S111-F12** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** allocTmpDir converts TMPDIR through String::from_utf8_lossy
  - **Oracle:** `src/os/file.zig:85-86` → **Rustty:** `crates/rustty-os/src/file.rs:146-147`
  - **Audit's suggested fix:** Return `Vec<u8>` (or OsString) trimmed of trailing '/' bytes.
- [x] **S111-F2** · CONFIRMED · `ABI` · stub-or-unimplemented
  - **What:** gerror_message() is a stub returning the literal "GError" — GLib error text is never logged
  - **Oracle:** `src/os/flatpak.zig:254,274,297,304,326-337,398` → **Rustty:** `crates/rustty-os/src/flatpak.rs:891-895`
  - **Audit's suggested fix:** Declare `#[repr(C)] struct GError { domain: u32, code: c_int, message: *const c_char }` and read `(*err).message` (null-checked) into the log string.
- [ ] **S111-F4** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** All cf_release_thread log calls are empty no-op helpers (warn/err/debug never emitted)
  - **Oracle:** `src/os/cf_release_thread.zig:105,112-113,116,122,137-138,169,178` → **Rustty:** `crates/rustty-os/src/cf_release_thread.rs:297-328`
  - **Audit's suggested fix:** Emit the messages through the crate's eprintln!-based scoped log convention (`[warn] [cf_release_thread] ...`) as file.rs/edit.rs do, and add the `cf release thread exited` debug line at the end of threadMain_.
- [ ] **S111-F5** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** All locale.zig log.info/log.warn/log.debug sites are empty helpers or comments — nothing is emitted
  - **Oracle:** `src/os/locale.zig:31,45,60,71,74,76,87,98,114,117,121,134,139-142,188` → **Rustty:** `crates/rustty-os/src/locale.rs:143-161,242,261,286,290,294,306,315,364`
  - **Audit's suggested fix:** Route these through the crate's eprintln!-based scoped log helpers (`[warn] [os_locale] ...`).
- [ ] **S111-F6** · CONFIRMED · `INDIV` · missing-error-path
  - **What:** preferredLanguageFromCocoa no longer bounds the joined LANGUAGE string by the 1024-byte buffer
  - **Oracle:** `src/os/locale.zig:183-189,193-196,208,215` → **Rustty:** `crates/rustty-os/src/locale.rs:352-392`
  - **Audit's suggested fix:** Track the logical write position against `buf.len()` (including the trailing NUL) and return NoSpaceLeft on overflow; pass bytes to canonicalizeLocale instead of requiring &str.
- [ ] **S111-F7** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** getKernelInfo returns None when osrelease exceeds 128 bytes; oracle truncates to 128
  - **Oracle:** `src/os/kernel_info.zig:12-18` → **Rustty:** `crates/rustty-os/src/kernel_info.rs:22-28`
  - **Audit's suggested fix:** Truncate to the first 128 bytes before trimming instead of returning None.
- [ ] **S111-F8** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** cgroup::current does a single read() where the oracle's readSliceShort fills buf until EOF
  - **Oracle:** `src/os/cgroup.zig:18-22` → **Rustty:** `crates/rustty-os/src/cgroup.rs:30-34`
  - **Audit's suggested fix:** Loop on read() until 0 or buf is full (or use read_exact-style filling that stops at EOF).
- [ ] **S111-F9** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** FlatpakHostCommand.argv is Vec<String> instead of byte slices
  - **Oracle:** `src/os/flatpak.zig:50,343-348` → **Rustty:** `crates/rustty-os/src/flatpak.rs:261,667-671`
  - **Audit's suggested fix:** Use `Vec<Vec<u8>>` and `CString::new(arg.as_slice())` as is already done for env keys/values.
