#!/usr/bin/env python3
"""P3-W7 T4 Step 1 — delete `crates/rustty-os/src/args.rs` (upstream deleted
`src/os/args.zig`, −130) and repoint the consumer seam at `std::env::args_os()`.

Upstream's `ArgIterator` used `NSProcessInfo.arguments` via objc on macOS
because under `NSApplicationMain` the libc argv pointers Zig captures at startup
are invalid. **The Rust motivation never existed:** `std::env::args_os()` on
macOS reads the real process argv (via `_NSGetArgv`) and is valid under
`NSApplicationMain`. Keeping `IteratorMacOS` meant an `objc2`/`objc2-foundation`
path that is also LOSSY — `String::from_utf8_lossy(bytes).into_owned()` at
`args.rs:172` corrupted a non-UTF-8 argv element (a real concern for
`rustty -e <path with invalid UTF-8>`).
"""
import sys

EDITS = {
    "crates/rustty-args/src/args.rs": [
        (
            """/// Create an args iterator for the process args. This will skip argv0.
// pub fn argsIterator(alloc_gpa: Allocator) internal_os.args.ArgIterator.InitError!ArgsIterator(internal_os.args.ArgIterator) {
pub fn args_iterator() -> Result<ArgsIterator<OsArgIterator>, internal_os::args::InitError> {
    // var iter = try internal_os.args.iterator(alloc_gpa);
    let mut iter = OsArgIterator(internal_os::args::iterator()?);
    // errdefer iter.deinit();
    // _ = iter.next(); // skip argv0
    let _ = iter.next();
    // return .{ .iterator = iter };
    Ok(ArgsIterator::new(iter))
}

/// PORTS: `internal_os.args.ArgIterator` wrapped as a `GenIter`. The os iterator
/// yields `Option<String>`; this adapts it to `Option<Vec<u8>>` (byte-slices).
pub struct OsArgIterator(internal_os::args::ArgIterator);

impl GenIter for OsArgIterator {
    fn next(&mut self) -> Option<Vec<u8>> {
        self.0.next().map(|s| s.into_bytes())
    }
    fn deinit(&mut self) {
        self.0.deinit();
    }
}""",
            """/// PORT NOTE (P3-W7 T4): `internal_os.args.ArgIterator.InitError` — the
/// reference's `Allocator.Error`. `src/os/args.zig` was DELETED upstream
/// (`e8525c0fd`), so the uninhabited init-error type moves here with the seam.
/// Allocation failure aborts in Rust rather than being recoverable, so the
/// error set is genuinely empty and every `?` on it is discharged by
/// `match e {}`.
#[derive(Debug, PartialEq, Eq)]
pub enum InitError {}

/// Create an args iterator for the process args. This will skip argv0.
// pub fn argsIterator(alloc_gpa: Allocator) internal_os.args.ArgIterator.InitError!ArgsIterator(internal_os.args.ArgIterator) {
pub fn args_iterator() -> Result<ArgsIterator<OsArgIterator>, InitError> {
    // var iter = try internal_os.args.iterator(alloc_gpa);
    let mut iter = OsArgIterator::new();
    // errdefer iter.deinit();
    // _ = iter.next(); // skip argv0
    let _ = iter.next();
    // return .{ .iterator = iter };
    Ok(ArgsIterator::new(iter))
}

/// PORTS: the process argument iterator, wrapped as a `GenIter` yielding
/// `Option<Vec<u8>>` (byte-slices), which is what the reference's
/// `std.process.ArgIterator` yields (`?[]const u8`).
///
/// PORT NOTE (P3-W7 T4): this used to wrap `rustty_os::args::ArgIterator`,
/// whose macOS arm went through `NSProcessInfo.arguments` and
/// `String::from_utf8_lossy` — so a non-UTF-8 argv element was CORRUPTED.
/// Upstream deleted `src/os/args.zig` outright; on the Rust side the objc
/// detour never had a reason to exist, because `std::env::args_os()` on macOS
/// reads the real process argv (via `_NSGetArgv`) and is valid under
/// `NSApplicationMain`. `OsStr::as_encoded_bytes` is lossless on every
/// platform.
pub struct OsArgIterator(std::env::ArgsOs);

impl OsArgIterator {
    #[must_use]
    pub fn new() -> Self {
        // if (true) return try std.process.argsWithAllocator(allocator);
        OsArgIterator(std::env::args_os())
    }
}

impl Default for OsArgIterator {
    fn default() -> Self {
        Self::new()
    }
}

impl GenIter for OsArgIterator {
    fn next(&mut self) -> Option<Vec<u8>> {
        self.0.next().map(|s| s.as_encoded_bytes().to_vec())
    }
}""",
        ),
    ],
    "crates/rustty-cli/src/action.rs": [
        (
            """    let mut iter = OsArgs(rustty_os::args::iterator().unwrap_or_else(|e| match e {}));
    // return try detectIter(E, &iter);
    let result = detect_iter::<E, _>(&mut iter);
    iter.0.deinit();
    result
}

/// Adapter: `rustty_os::args::ArgIterator` (yields `Option<String>`) viewed as an
/// [`ArgsIter`] over byte-slices, mirroring the reference's `std.process
/// .ArgIterator` yielding `?[]const u8`. Note the reference iterates ALL args
/// (including argv0) — `detectArgs` does not skip argv0 — which never starts with
/// `+` and never matches a special case, so it is harmless.
struct OsArgs(rustty_os::args::ArgIterator);

impl ArgsIter for OsArgs {
    fn next(&mut self) -> Option<Vec<u8>> {
        self.0.next().map(String::into_bytes)
    }
}""",
            """    let mut iter = OsArgs(std::env::args_os());
    // return try detectIter(E, &iter);
    detect_iter::<E, _>(&mut iter)
}

/// Adapter: the process argument iterator viewed as an [`ArgsIter`] over
/// byte-slices, mirroring the reference's `std.process.ArgIterator` yielding
/// `?[]const u8`. Note the reference iterates ALL args (including argv0) —
/// `detectArgs` does not skip argv0 — which never starts with `+` and never
/// matches a special case, so it is harmless.
///
/// PORT NOTE (P3-W7 T4): this used to wrap `rustty_os::args::ArgIterator`
/// (deleted with `src/os/args.zig`), whose `Option<String>` yield corrupted a
/// non-UTF-8 argv element. `OsStr::as_encoded_bytes` is lossless.
struct OsArgs(std::env::ArgsOs);

impl ArgsIter for OsArgs {
    fn next(&mut self) -> Option<Vec<u8>> {
        self.0.next().map(|s| s.as_encoded_bytes().to_vec())
    }
}""",
        ),
    ],
    "crates/rustty-cli/src/rustty.rs": [
        (
            """    ArgsInit(rustty_os::args::InitError),""",
            """    ArgsInit(rustty_args::args::InitError),""",
        ),
        (
            """impl From<rustty_os::args::InitError> for Error {
    fn from(e: rustty_os::args::InitError) -> Self {""",
            """impl From<rustty_args::args::InitError> for Error {
    fn from(e: rustty_args::args::InitError) -> Self {""",
        ),
    ],
}

for path, edits in EDITS.items():
    src = open(path, encoding="utf-8").read()
    for old, new in edits:
        if old not in src:
            print(f"MISS in {path}: {old.splitlines()[0][:70]}", file=sys.stderr)
            sys.exit(1)
        src = src.replace(old, new, 1)
    open(path, "w", encoding="utf-8").write(src)
    print("patched", path)
