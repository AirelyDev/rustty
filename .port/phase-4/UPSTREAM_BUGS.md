# Upstream bugs found while porting the phase-4 range

Modeled on `.port/phase-3/UPSTREAM_BUGS.md`. Phase-3's entries are all
*faithfully ported* defects (rustty wrong in the same way as Ghostty, on
purpose). Phase-4 adds a second kind, used only under an explicit, ratified
escalation ruling: an upstream **regression that was deliberately NOT ported**,
recorded here so the divergence from the oracle end state is documented rather
than silent (R12 doctrine: divergence needs documentation either way). Each
entry states which kind it is, quotes the exact upstream lines, and needs an
upstream report; if upstream fixes it, the entry is closed by a normal port.

---

## P4-U1 — `crash/sentry.zig` `cacheDir` macOS XDG_CACHE_HOME check INVERTED (NOT ported)

**Kind:** upstream regression, deliberately NOT ported (binding escalation
ruling in the w6-crash task; assessment row P4-E35).

**Introduced by:** `15c50c1db` "crash: do not use global state" (2026-08-02),
while extracting the inline cache-dir block into the file-local `cacheDir`.
`a82637b53` "crash: resolve sentry directories on the init thread"
(2026-08-09) only re-threaded the call (`cacheDir(single_threaded.io(), gpa,
&environ_map)`) and did not touch the condition, so the inversion survives to
the range head `12967b68f`.

**Pre-range behavior** (deleted by `15c50c1db`, `src/crash/sentry.zig`
`initThread`; matches the comment):

```zig
        // On macOS, we prefer to use the NSCachesDirectory value to be
        // a more idiomatic macOS application. But if XDG env vars are set
        // we will respect them.
        if (comptime builtin.os.tag == .macos) macos: {
            if (global.environ().containsUnemptyConstant("XDG_CACHE_HOME")) break :macos;
            break :cache_dir try internal_os.macos.cacheDir(
                alloc,
                "sentry",
            );
        }
```

i.e. `XDG_CACHE_HOME` set-and-nonempty → break out of the macOS arm → XDG
cache path; unset or empty → `NSCachesDirectory`.

**At range head `12967b68f`** (`src/crash/sentry.zig:188-200`, working tree
verified):

```zig
fn cacheDir(io: std.Io, alloc: Allocator, environ_map: *const std.process.Environ.Map) ![]const u8 {
    // On macOS, we prefer to use the NSCachesDirectory value to be
    // a more idiomatic macOS application. But if XDG env vars are set
    // we will respect them.
    if (comptime builtin.os.tag == .macos) macos: {
        const xdg_cache_home = environ_map.get("XDG_CACHE_HOME") orelse break :macos;
        if (xdg_cache_home.len > 0) {
            return try internal_os.macos.cacheDir(
                alloc,
                "sentry",
            );
        }
    }
```

i.e. `XDG_CACHE_HOME` set-and-nonempty → **`NSCachesDirectory`**; unset (or
set-empty, falling through the `if`) → **XDG cache path**. That is the exact
opposite of the pre-range logic for every case, and the retained comment
("But if XDG env vars are set we will respect them") now contradicts the code
on the very next lines — the tell that this is an accidental inversion, not an
intended behavior change (no commit-message mention either).

**What rustty does** (`crates/rustty-crash/src/sentry.rs`, `fn cacheDir`):
ports the `15c50c1db`/`a82637b53` restructure faithfully (file-local
`cacheDir` over the caller-supplied env-map snapshot, all resolution on the
init thread) but keeps the pre-range, comment-consistent condition:
set-and-nonempty `XDG_CACHE_HOME` → respect XDG (`xdg.cache`); unset or empty
→ `NSCachesDirectory` (`""` treated as unset, preserving the old
`containsUnemptyConstant` semantics). An inline `UPSTREAM BUG` note at the
condition points back to this entry.

**Consequence of the upstream bug if it had been ported:** on macOS, users who
export `XDG_CACHE_HOME` to redirect caches would get their Sentry database in
`~/Library/Caches` anyway, while default-configured users would suddenly get
`~/.cache/ghostty/sentry` — a silent cache-location swap for everyone.

**Upstream report:** to be filed against ghostty (`cacheDir` comment/code
contradiction introduced by 15c50c1db). Once upstream fixes it (in either
direction), re-sync this fn and close this entry.
