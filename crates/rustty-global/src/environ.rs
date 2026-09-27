//! rustty-global::environ
//!
//! PORTS: the environment-block half of ghostty `src/global.zig` at
//! `e8525c0fd`/`a77c706a1`/`7121ab6c3`/`048619a6b` — the `environ` field of
//! `GlobalState`, the `environ()` / `environMap()` / `syncEnviron()` free
//! accessors, and the `tmp_dir_path` field with its `tmpDirPath()` accessor.
//!
//! PORT STATUS: ported (P3-W7 T1 / area-E `E16`)
//!
//! # What upstream did
//!
//! Upstream captures the process environment **once** at `global.init` (from
//! `std.process.Init.Minimal.environ` for the `main`/`tool` arms, or from the C
//! caller for the `.c` arm) and threads it explicitly everywhere else, instead
//! of reading the process environment ad-hoc. `os/env.zig` — which was the
//! ad-hoc reader — was deleted in the same commit.
//!
//! # PORT NOTE: `std.process.Environ` -> `Environ`
//!
//! Zig's `std.process.Environ` is a `[*:null]?[*:0]u8` block of `KEY=VALUE`
//! entries plus a length. Its accessors are `getPosix(key) ?[:0]const u8`
//! (a borrow into the block, POSIX only), `getAlloc(alloc, key) ![]u8` (a
//! copy, portable), and `createMap(alloc) !Map`.
//!
//! Zig byte-slices become `Vec<u8>`/`&[u8]`, never lossy `String`: environment
//! variables on POSIX are arbitrary byte strings, and a `String`-typed
//! environment is exactly the translation defect that made
//! `rustty-os`'s `getEnvMap` panic on a non-UTF-8 variable. On Windows the
//! environment is WTF-16; `OsStr::as_encoded_bytes` gives a **lossless**
//! (WTF-8) byte view on every platform, which is what this module stores, so
//! the byte typing is portable rather than POSIX-only.
//!
//! # PORT NOTE: where the process-global lives
//!
//! Upstream reads `state.?.environ`. rustty's `GlobalState` is **generic** over
//! the two deferred-seam types (`Action`, `ResourcesDir`, break #1/R4) and is
//! therefore instantiated at two different bindings —
//! `rustty_global::STATE` (`Action = ()`, `ResourcesDir = ()`, used by
//! `rustty-termio`) and `rustty_core::STATE` (the real `ResourcesDir`, used by
//! the app). A free accessor cannot name "the" state. So the environment block
//! and the temp dir — both of which are **non-generic** — are held in
//! module-level process-globals here, which is the single place both bindings'
//! initializers publish into. `GlobalState` still carries the fields for
//! fidelity, and `GlobalState::new` publishes them.
//!
//! # PORT NOTE: `builtin.is_test` -> lazy capture
//!
//! Every upstream accessor has an `if (builtin.is_test) return <testing fallback>`
//! arm. `builtin.is_test` is whole-compilation in Zig; Rust's `cfg(test)` is
//! **not transitive** (a crate built as a dependency of a test binary is built
//! without `cfg(test)`), so that arm cannot be ported literally — see
//! `.port/phase-3/REVIEW_CLASSES.md` "cfg(test) non-transitivity". The faithful
//! equivalent is a lazy capture of the real process environment when the state
//! has not been initialized: that is what upstream's testing fallback
//! (`std.testing.environ`) is, and it keeps `#[test]` code and library
//! consumers working without an initialized global.

use indexmap::map::MutableKeys as _;
use indexmap::{Equivalent, IndexMap};
use std::ffi::{OsStr, OsString};
use std::sync::{OnceLock, RwLock, RwLockReadGuard};

/// PORT NOTE: `std.process.Environ.Map` — the owned key/value map built off the
/// captured block. Byte-typed for the same reason `Environ` is.
///
/// Upstream's `Map` is an `ArrayHashMap`: it iterates in INSERTION order, and
/// that order is observable, because `Command` builds the child's `envp` by
/// iterating the map (`createNullDelimitedEnvMap`), so a child sees its
/// `environ` in that order. The port keeps it (an `IndexMap`): `put`
/// appends a new name and leaves an existing one where it is, `putMove` too,
/// `orderedRemove` ([`EnvMap::remove`]) shifts the later entries down and
/// `swapRemove` ([`EnvMap::swap_remove`]) moves the last entry into the hole.
/// The reference uses `swapRemove` for `GHOSTTY_BASH_ENV`
/// (`shell_integration.zig:382`) and `orderedRemove` everywhere else.
/// CORRECTION (N384, task W-5b): this was a `HashMap` with a random iteration
/// order and one `remove`, and the note said the order was not observable.
///
/// PORT NOTE (N340, task W-5): the key comparison is the reference map's
/// `EnvNameHashContext` (`std/process/Environ.zig:107-142`): byte equality
/// everywhere except Windows, where environment variable names are
/// case-insensitive and the map hashes and compares keys by
/// `std.os.windows.toUpperWtf16` over their WTF-8 code points
/// (`eqlIgnoreCaseWtf8`, `std/os/windows.zig:3754`). So on Windows `get("PATH")`
/// finds the `Path` the system environment spells. CORRECTION: this was a
/// plain `HashMap<Vec<u8>, Vec<u8>>`, case-sensitive on every target.
///
/// `insert` is the reference's `put`: an existing entry keeps its key spelling
/// and takes the new value. [`EnvMap::put_move`] is `putMove`, which also
/// replaces the stored key (what `putWindowsBlock` uses). Lookups take a
/// borrowed key and do not allocate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvMap {
    map: IndexMap<EnvKey, Vec<u8>>,
}

/// A key of [`EnvMap`], compared as the reference's `EnvNameHashContext` does.
#[derive(Clone, Debug)]
struct EnvKey(Vec<u8>);

/// A borrowed [`EnvKey`] for lookups: the same hash and the same comparison.
struct EnvKeyRef<'a>(&'a [u8]);

impl PartialEq for EnvKey {
    fn eq(&self, other: &Self) -> bool {
        eql_keys(&self.0, &other.0)
    }
}

impl Eq for EnvKey {}

impl std::hash::Hash for EnvKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        hash_key(&self.0, state);
    }
}

impl std::hash::Hash for EnvKeyRef<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        hash_key(self.0, state);
    }
}

impl Equivalent<EnvKey> for EnvKeyRef<'_> {
    fn equivalent(&self, key: &EnvKey) -> bool {
        eql_keys(self.0, &key.0)
    }
}

/// `EnvNameHashContext.hash` (`std/process/Environ.zig:108-129`).
fn hash_key<H: std::hash::Hasher>(s: &[u8], state: &mut H) {
    // pub fn hash(self: @This(), s: []const u8) u32 {
    //     switch (native_os) {
    //         else => return std.array_hash_map.hashString(s),
    //         .windows => {
    //             var h = std.hash.Wyhash.init(0);
    //             var it = unicode.Wtf8View.initUnchecked(s).iterator();
    //             while (it.nextCodepoint()) |cp| {
    //                 const cp_upper = if (std.math.cast(u16, cp)) |wtf16|
    //                     std.os.windows.toUpperWtf16(wtf16)
    //                 else
    //                     cp;
    //                 h.update(&[_]u8{ ... cp_upper's three low bytes ... });
    //             }
    //             return @truncate(h.final());
    //         },
    //     }
    // }
    // PORT NOTE: the hasher is Rust's; only the hash/equality agreement is
    // load-bearing, and both folds are the reference's.
    #[cfg(windows)]
    {
        for cp in wtf8_code_points(s) {
            let cp_upper = match u16::try_from(cp) {
                Ok(wtf16) => u32::from(to_upper_wtf16(wtf16)),
                Err(_) => cp,
            };
            state.write_u32(cp_upper);
        }
    }
    #[cfg(not(windows))]
    {
        std::hash::Hash::hash(s, state);
    }
}

/// `fn eqlKeys(a: []const u8, b: []const u8) bool`
/// (`std/process/Environ.zig:137-142`).
fn eql_keys(a: &[u8], b: &[u8]) -> bool {
    // return switch (native_os) {
    //     else => std.array_hash_map.eqlString(a, b),
    //     .windows => std.os.windows.eqlIgnoreCaseWtf8(a, b),
    // };
    #[cfg(windows)]
    {
        eql_ignore_case_wtf8(a, b)
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

/// PORTS: `std.os.windows.toUpperWtf16(c)` (`std/os/windows.zig:3716-3720`),
/// whose Windows arm is `ntdll.RtlUpcaseUnicodeChar`: the port binds the same
/// entry point (as `rustty-config`'s `path.rs` does).
#[cfg(windows)]
fn to_upper_wtf16(c: u16) -> u16 {
    #[link(name = "ntdll")]
    extern "system" {
        fn RtlUpcaseUnicodeChar(SourceCharacter: u16) -> u16;
    }
    // SAFETY: `RtlUpcaseUnicodeChar` takes and returns a WCHAR by value and
    // reads only the system's upcase table.
    unsafe { RtlUpcaseUnicodeChar(c) }
}

/// PORTS: `std.os.windows.eqlIgnoreCaseWtf8(a, b)` (`std/os/windows.zig:3750-3779`).
/// Compares two WTF-8 strings using the equivalent functionality of
/// `RtlEqualUnicodeString` (with case insensitive comparison enabled).
#[cfg(windows)]
fn eql_ignore_case_wtf8(a: &[u8], b: &[u8]) -> bool {
    // A length equality check is not possible here because there are
    // some codepoints that have a different length uppercase UTF-8 representations
    // than their lowercase counterparts, e.g. U+0250 (2 bytes) <-> U+2C6F (3 bytes).
    // There are 7 such codepoints in the uppercase data used by Windows.
    let a_it = wtf8_code_points(a);
    let mut b_it = wtf8_code_points(b);
    // while (true) { const a_cp = a_wtf8_it.nextCodepoint() orelse break; ... }
    for a_cp in a_it {
        let Some(b_cp) = b_it.next() else {
            return false;
        };
        match (u16::try_from(a_cp), u16::try_from(b_cp)) {
            (Ok(a16), Ok(b16)) => {
                if a_cp != b_cp && to_upper_wtf16(a16) != to_upper_wtf16(b16) {
                    return false;
                }
            }
            _ => {
                if a_cp != b_cp {
                    return false;
                }
            }
        }
    }
    // Make sure there are no leftover codepoints in b
    b_it.next().is_none()
}

/// PORTS: `std.unicode.Wtf8View.initUnchecked(s).iterator()`'s
/// `nextCodepoint` stream.
///
/// PORT NOTE: the reference only ever sees valid WTF-8 here (`Map.get`/`put`
/// assert `validateKeyForFetch`/`validateKeyForPut`, and every key the port
/// stores comes from `OsStr::as_encoded_bytes`, which is WTF-8 on Windows, or
/// from a literal). The port does not trap on an invalid sequence: each byte of
/// one yields `0x110000 + byte`, a value above every code point, so it is
/// compared exactly and hashes consistently with that comparison.
#[cfg(windows)]
fn wtf8_code_points(mut s: &[u8]) -> impl Iterator<Item = u32> + '_ {
    std::iter::from_fn(move || {
        let (&first, rest) = s.split_first()?;
        let len = match first {
            0x00..=0x7f => 1,
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => 0,
        };
        let valid = len > 0
            && s.len() >= len
            && s[1..len].iter().all(|&c| c & 0xc0 == 0x80)
            && match len {
                3 => !(first == 0xe0 && s[1] < 0xa0),
                4 => !((first == 0xf0 && s[1] < 0x90) || (first == 0xf4 && s[1] > 0x8f)),
                _ => true,
            };
        if !valid {
            s = rest;
            return Some(0x11_0000 + u32::from(first));
        }
        let cp = match len {
            1 => u32::from(first),
            2 => (u32::from(first & 0x1f) << 6) | u32::from(s[1] & 0x3f),
            3 => {
                (u32::from(first & 0x0f) << 12)
                    | (u32::from(s[1] & 0x3f) << 6)
                    | u32::from(s[2] & 0x3f)
            }
            _ => {
                (u32::from(first & 0x07) << 18)
                    | (u32::from(s[1] & 0x3f) << 12)
                    | (u32::from(s[2] & 0x3f) << 6)
                    | u32::from(s[3] & 0x3f)
            }
        };
        s = &s[len..];
        Some(cp)
    })
}

impl EnvMap {
    /// `Map.init(allocator)`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty map with room for `capacity` entries.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: IndexMap::with_capacity(capacity),
        }
    }

    /// `pub fn put(self: *Map, key, value)`: `key` and `value` are copied into
    /// the map; an existing entry keeps its key and its position and takes the
    /// new value, a new one is appended. Returns the replaced value.
    pub fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) -> Option<Vec<u8>> {
        self.map.insert(EnvKey(key), value)
    }

    /// `pub fn putMove(self: *Map, key, value)`: like [`EnvMap::insert`], but an
    /// existing entry's key is replaced by `key` too (in place: `getOrPut` then
    /// `key_ptr.* = key`, so the entry keeps its position).
    pub fn put_move(&mut self, key: Vec<u8>, value: Vec<u8>) {
        if let Some((_, k, v)) = self.map.get_full_mut2(&EnvKeyRef(&key)) {
            *k = EnvKey(key);
            *v = value;
        } else {
            self.map.insert(EnvKey(key), value);
        }
    }

    /// `pub fn get(self: Map, key) ?[]const u8`.
    #[must_use]
    pub fn get<K: AsRef<[u8]> + ?Sized>(&self, key: &K) -> Option<&Vec<u8>> {
        self.map.get(&EnvKeyRef(key.as_ref()))
    }

    /// `pub fn getPtr(self: Map, key) ?*[]const u8`.
    pub fn get_mut<K: AsRef<[u8]> + ?Sized>(&mut self, key: &K) -> Option<&mut Vec<u8>> {
        self.map.get_mut(&EnvKeyRef(key.as_ref()))
    }

    /// `pub fn contains(m: *const Map, key) bool`.
    #[must_use]
    pub fn contains_key<K: AsRef<[u8]> + ?Sized>(&self, key: &K) -> bool {
        self.map.contains_key(&EnvKeyRef(key.as_ref()))
    }

    /// `pub fn orderedRemove(self: *Map, key) bool`: the later entries move
    /// down one place. Returns the removed value.
    pub fn remove<K: AsRef<[u8]> + ?Sized>(&mut self, key: &K) -> Option<Vec<u8>> {
        self.map.shift_remove(&EnvKeyRef(key.as_ref()))
    }

    /// `pub fn swapRemove(self: *Map, key) bool`: the last entry takes the
    /// removed one's place. Returns the removed value.
    pub fn swap_remove<K: AsRef<[u8]> + ?Sized>(&mut self, key: &K) -> Option<Vec<u8>> {
        self.map.swap_remove(&EnvKeyRef(key.as_ref()))
    }

    /// `pub fn count(self: Map) Size`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// `count() == 0`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// `pub fn iterator(self: *const Map)`: every entry, with its stored key,
    /// in the map's order (see the type's PORT NOTE).
    pub fn iter(&self) -> impl Iterator<Item = (&Vec<u8>, &Vec<u8>)> + '_ {
        self.map.iter().map(|(k, v)| (&k.0, v))
    }

    /// `pub fn keys(map: *const Map)`.
    pub fn keys(&self) -> impl Iterator<Item = &Vec<u8>> + '_ {
        self.map.keys().map(|k| &k.0)
    }

    /// `pub fn values(map: *const Map)`.
    pub fn values(&self) -> impl Iterator<Item = &Vec<u8>> + '_ {
        self.map.values()
    }
}

impl<'a> IntoIterator for &'a EnvMap {
    type Item = (&'a Vec<u8>, &'a Vec<u8>);
    type IntoIter = Box<dyn Iterator<Item = (&'a Vec<u8>, &'a Vec<u8>)> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl IntoIterator for EnvMap {
    type Item = (Vec<u8>, Vec<u8>);
    type IntoIter = Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)>>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.map.into_iter().map(|(k, v)| (k.0, v)))
    }
}

impl FromIterator<(Vec<u8>, Vec<u8>)> for EnvMap {
    fn from_iter<I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>>(iter: I) -> Self {
        let mut map = Self::new();
        map.extend(iter);
        map
    }
}

impl Extend<(Vec<u8>, Vec<u8>)> for EnvMap {
    fn extend<I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

impl<const N: usize> From<[(Vec<u8>, Vec<u8>); N]> for EnvMap {
    fn from(entries: [(Vec<u8>, Vec<u8>); N]) -> Self {
        entries.into_iter().collect()
    }
}

/// PORT NOTE (P3-W7 T2): `std.process.EnvMap`'s own API is
/// `put(key, value)` / `get(key)` / `remove(key)` / `getPtr(key)` over
/// `[]const u8`, i.e. byte slices. rustty's `EnvMap` (once a plain `HashMap`,
/// whose inherent `insert` wants owned `Vec<u8>` keys) spells them
/// `insert`/`get`/`remove`. This extension trait restores the reference's own spelling AND
/// keeps every call site byte-typed, so a non-UTF-8 key or value round-trips
/// instead of panicking (the defect this task removes).
pub trait EnvMapExt {
    /// PORT NOTE: `try env.put(key, value)`.
    fn put(&mut self, key: &[u8], value: &[u8]);
    /// PORT NOTE: `env.get(key)`.
    fn getv(&self, key: &[u8]) -> Option<&[u8]>;
    /// PORT NOTE: `_ = env.orderedRemove(key)`. The one `swapRemove` caller
    /// uses [`EnvMap::swap_remove`] (see the type's PORT NOTE). CORRECTION
    /// (N384, task W-5b): this said the order was not observable.
    fn removev(&mut self, key: &[u8]);
    /// PORT NOTE: `env.get(key) != null`.
    fn hasv(&self, key: &[u8]) -> bool;
}

impl EnvMapExt for EnvMap {
    fn put(&mut self, key: &[u8], value: &[u8]) {
        self.insert(key.to_vec(), value.to_vec());
    }
    fn getv(&self, key: &[u8]) -> Option<&[u8]> {
        EnvMap::get(self, key).map(Vec::as_slice)
    }
    fn removev(&mut self, key: &[u8]) {
        EnvMap::remove(self, key);
    }
    fn hasv(&self, key: &[u8]) -> bool {
        EnvMap::contains_key(self, key)
    }
}

/// PORT NOTE: `std.process.Environ` — the process environment block.
///
/// On POSIX upstream borrows into the live process block (`PosixBlock`);
/// rustty owns a copy, because Rust has no way to hand out a `'static` borrow
/// of `**environ` that stays sound across a `setenv`. That is why
/// `sync_environ` re-captures it, as the reference re-points its block.
///
/// On Windows the reference's `Block` is `GlobalBlock` (`std/process/Environ.zig:25-46`):
/// "the memory pointed at by the PEB changes when the environment is modified,
/// so a long-lived pointer cannot be used", so the environment holds only
/// `use_global` and every lookup (`createMap`, `getWindows`, and through them
/// `getAlloc`/`containsConstant`) reads the LIVE process environment. The
/// port does the same: [`Environ::from_process`] is `.{ .use_global = true }`
/// on Windows and reads the process environment on each lookup. CORRECTION
/// (N352, task W-5): the port captured a snapshot on Windows too, so a
/// variable set after the capture stayed invisible (and `syncEnviron`, a
/// no-op on Windows, could not refresh it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environ {
    /// The `KEY=VALUE` entries, in block order. On Windows these are the
    /// entries of an explicit block ([`Environ::from_entries`]); a
    /// `use_global` environment keeps none and reads the process instead.
    entries: Vec<Vec<u8>>,
    /// `GlobalBlock.use_global` (Windows): `true` for the process environment,
    /// `false` for `GlobalBlock.empty` and for an explicit block.
    #[cfg(windows)]
    use_global: bool,
}

impl Environ {
    /// Capture the current process environment.
    ///
    /// PORT NOTE: this is `std.process.Init.Minimal.environ` (the `main` arm of
    /// upstream's `InitOpts`) — the environment as the process was started
    /// with, plus any in-process mutation up to this point. Uses `vars_os`, NOT
    /// `vars`: `std::env::vars` **panics** on a non-UTF-8 variable and POSIX
    /// environment variables are arbitrary byte strings.
    ///
    /// On Windows nothing is captured: this is `.{ .block = .global }`, and
    /// each lookup reads the live environment (see the type's PORT NOTE).
    #[must_use]
    pub fn from_process() -> Self {
        #[cfg(windows)]
        {
            Self {
                entries: Vec::new(),
                use_global: true,
            }
        }
        #[cfg(not(windows))]
        {
            Self {
                entries: process_entries(),
            }
        }
    }

    /// Build an `Environ` from raw `KEY=VALUE` entries.
    ///
    /// PORT NOTE: upstream's `.c` arm takes the block straight from the C
    /// caller (`global.init(.{ .c = .{ .environ = ... } })`). On Windows the
    /// reference has no explicit block (its `.c` arm passes `.use_global = true`,
    /// `main_c.zig:127`); the port keeps this constructor there for tests, and
    /// reads such a block with the Windows lookup rules (case-insensitive keys).
    #[must_use]
    pub fn from_entries(entries: Vec<Vec<u8>>) -> Self {
        Self {
            entries,
            #[cfg(windows)]
            use_global: false,
        }
    }

    /// The raw `KEY=VALUE` entries in block order.
    ///
    /// PORT NOTE: POSIX only, like the reference's `PosixBlock.view()`; a Windows
    /// environment is read live.
    #[cfg(not(windows))]
    #[must_use]
    pub fn entries(&self) -> &[Vec<u8>] {
        &self.entries
    }

    /// PORT NOTE: `pub fn getPosix(self: Environ, key: []const u8) ?[:0]const u8`
    /// — a borrow into the block, no allocation. Returns the FIRST match, which
    /// is what `getenv(3)` does. POSIX only, as in the reference (a
    /// `GlobalBlock` has no `view()`).
    #[cfg(not(windows))]
    #[must_use]
    #[allow(non_snake_case)]
    pub fn get_posix(&self, key: &[u8]) -> Option<&[u8]> {
        for entry in &self.entries {
            let Some(eq) = entry.iter().position(|&b| b == b'=') else {
                continue;
            };
            if &entry[..eq] == key {
                return Some(&entry[eq + 1..]);
            }
        }
        None
    }

    /// PORT NOTE: `pub fn getWindows(environ: Environ, key) ?[:0]const u16`
    /// (`std/process/Environ.zig:621-666`): the FIRST entry whose name equals
    /// `key` case-insensitively (`eqlIgnoreCaseWtf16`, i.e.
    /// `RtlEqualUnicodeString`), read from the live environment. The value is
    /// returned owned (WTF-8) because the live block cannot be borrowed.
    #[cfg(windows)]
    #[must_use]
    pub fn get_windows(&self, key: &[u8]) -> Option<Vec<u8>> {
        // '=' anywhere but the start makes this an invalid environment variable name.
        if key.is_empty() || key[1..].contains(&b'=') {
            return None;
        }

        for entry in self.windows_block().iter() {
            // There are some special environment variables that start with =,
            // so we need a special case to not treat = as a key/value separator
            // if it's the first character.
            // https://devblogs.microsoft.com/oldnewthing/20100506-00/?p=14133
            let Some(equal_index) = entry.iter().skip(1).position(|&b| b == b'=').map(|p| p + 1)
            else {
                // The reference asserts `unreachable` here: CreateProcess
                // enforces the `=`. An explicit test block may lack it; the
                // port skips such an entry.
                continue;
            };

            let this_key = &entry[..equal_index];
            if eql_ignore_case_wtf8(key, this_key) {
                return Some(entry[equal_index + 1..].to_vec());
            }
        }
        None
    }

    /// The Windows block's entries: the live process environment for a
    /// `use_global` environment, none for `GlobalBlock.empty`, and the explicit
    /// entries otherwise.
    #[cfg(windows)]
    fn windows_block(&self) -> std::borrow::Cow<'_, [Vec<u8>]> {
        if self.use_global {
            std::borrow::Cow::Owned(process_entries())
        } else {
            std::borrow::Cow::Borrowed(&self.entries)
        }
    }

    /// PORT NOTE: `pub fn getAlloc(self: Environ, alloc, key) ![]u8` — the
    /// portable (Windows-safe) copying form. Upstream returns
    /// `error.EnvironmentVariableMissing` when absent; Rust's `Option` carries
    /// that.
    ///
    /// The reference's body on every target (`std/process/Environ.zig:689-695`):
    /// `createMap` and `map.get(key)`, so of two entries with the same name the
    /// LAST one is returned (`createMap` `put`s each entry), where `getPosix`
    /// returns the first. CORRECTION (N381, task W-5b): the POSIX arm was
    /// `getPosix`, the first entry.
    #[must_use]
    pub fn get_alloc(&self, key: &[u8]) -> Option<Vec<u8>> {
        // var map = createMap(environ, gpa) catch return error.OutOfMemory;
        // const val = map.get(key) orelse return error.EnvironmentVariableMissing;
        self.create_map().get(key).cloned()
    }

    /// PORT NOTE: `pub fn contains(environ: Environ, gpa, key) ContainsError!bool`
    /// (`std/process/Environ.zig:540-545`): `createMap` then `map.contains(key)`.
    /// Zig's `error.InvalidWtf8` (Windows, an invalid key) reads as absent.
    #[must_use]
    pub fn contains(&self, key: &[u8]) -> bool {
        self.create_map().contains_key(key)
    }

    /// PORT NOTE: `pub fn containsUnempty(environ: Environ, gpa, key)
    /// ContainsError!bool` (`std/process/Environ.zig:554-560`): through
    /// `createMap`, so the LAST of two same-named entries decides, where
    /// [`Environ::contains_unempty_constant`] (`getPosix`) takes the first.
    #[must_use]
    pub fn contains_unempty(&self, key: &[u8]) -> bool {
        self.create_map().get(key).is_some_and(|v| !v.is_empty())
    }

    /// PORT NOTE: `pub inline fn containsConstant(self: Environ, comptime key)
    /// bool` (`std/process/Environ.zig:570`) — the non-allocating existence
    /// probe. The `comptime key` exists upstream only so the Windows arm can
    /// `comptime`-widen the literal to WTF-16; the POSIX arm is exactly
    /// `getPosix(key) != null`, and rustty's block is already byte-oriented on
    /// every target, so the key stays a runtime `&[u8]`. The Windows arm is
    /// `getWindows(key) != null`.
    #[must_use]
    pub fn contains_constant(&self, key: &[u8]) -> bool {
        #[cfg(windows)]
        {
            self.get_windows(key).is_some()
        }
        #[cfg(not(windows))]
        {
            self.get_posix(key).is_some()
        }
    }

    /// PORT NOTE: `pub inline fn containsUnemptyConstant(self: Environ, comptime
    /// key) bool` (`std/process/Environ.zig:589`) — present AND non-empty. See
    /// [`Environ::contains_constant`] for why `key` is not `comptime` here.
    ///
    /// Used by `Config.zig:5183`'s CLI-environment heuristic (`TERM_PROGRAM`)
    /// and `crash/sentry.zig:113` (`XDG_CACHE_HOME`).
    #[must_use]
    pub fn contains_unempty_constant(&self, key: &[u8]) -> bool {
        #[cfg(windows)]
        {
            self.get_windows(key).is_some_and(|v| !v.is_empty())
        }
        #[cfg(not(windows))]
        {
            self.get_posix(key).is_some_and(|v| !v.is_empty())
        }
    }

    /// PORT NOTE: `pub fn createMap(self: Environ, alloc) !Map`.
    ///
    /// On Windows this is `putWindowsBlock` over the live environment (or the
    /// explicit block): each entry is `putMove`d, so a later entry replaces an
    /// earlier one whose name differs only in case, key spelling included.
    #[must_use]
    pub fn create_map(&self) -> EnvMap {
        #[cfg(windows)]
        {
            let block = self.windows_block();
            let mut map = EnvMap::with_capacity(block.len());
            for entry in block.iter() {
                // if (view.ptr[key_start] == '=') i += 1;
                let start = usize::from(entry.first() == Some(&b'='));
                // while (view.ptr[i] != 0 and view.ptr[i] != '=') : (i += 1) {}
                let i = entry[start..]
                    .iter()
                    .position(|&b| b == b'=')
                    .map_or(entry.len(), |p| p + start);
                let key = entry[..i].to_vec();
                // if (view.ptr[i] == '=') i += 1;
                let value = entry.get(i + 1..).unwrap_or(&[]).to_vec();
                // PORT NOTE: `putMove` asserts `validateKeyForPut` (a non-empty
                // name); an explicit test block can hold an empty one, which the
                // port skips instead of trapping.
                if key.is_empty() {
                    continue;
                }
                map.put_move(key, value);
            }
            map
        }
        #[cfg(not(windows))]
        {
            // `putPosixBlock` (`std/process/Environ.zig:184-196`): `map.put`
            // for every entry, so a later entry with the same name replaces the
            // value (the first entry's key and position stay; `:256-271`).
            // CORRECTION (N381, task W-5b): the first entry won here.
            let mut map = EnvMap::with_capacity(self.entries.len());
            for entry in &self.entries {
                // PORT NOTE: an entry with no `=` never comes from a real
                // `environ`; the port skips it (unchanged).
                let Some(eq) = entry.iter().position(|&b| b == b'=') else {
                    continue;
                };
                map.insert(entry[..eq].to_vec(), entry[eq + 1..].to_vec());
            }
            map
        }
    }
}

/// The process environment as `KEY=VALUE` entries, read now.
///
/// `std::env::vars_os` reads `environ` on POSIX and `GetEnvironmentStringsW`
/// (a copy of the PEB block, taken under the PEB lock as the reference's
/// `createMap` does) on Windows; its Windows iterator keeps the names that
/// start with `=` (`=C:`), like `putWindowsBlock`.
fn process_entries() -> Vec<Vec<u8>> {
    let mut entries = Vec::new();
    for (k, v) in std::env::vars_os() {
        let mut entry = os_bytes(&k).to_vec();
        entry.push(b'=');
        entry.extend_from_slice(os_bytes(&v));
        entries.push(entry);
    }
    entries
}

/// Lossless byte view of an `OsStr` (WTF-8 on Windows, the raw bytes on POSIX).
#[must_use]
fn os_bytes(s: &OsStr) -> &[u8] {
    s.as_encoded_bytes()
}

/// Rebuild an `OsString` from bytes produced by [`os_bytes`].
///
/// # Safety contract
///
/// `bytes` must have come from `OsStr::as_encoded_bytes` (or be a
/// substring split on an ASCII boundary of such a value), which is the case
/// for every entry in an [`Environ`]: they are captured from `vars_os`, split
/// on the ASCII `'='`, or supplied by a caller that is contractually handing us
/// OS strings.
#[must_use]
pub fn os_string_from_bytes(bytes: &[u8]) -> OsString {
    // SAFETY: see the contract above.
    unsafe { OsString::from_encoded_bytes_unchecked(bytes.to_vec()) }
}

/// The process-global captured environment.
///
/// PORT NOTE: `state.?.environ`. See the module docs for why this is not a
/// `GlobalState` field lookup.
static ENVIRON: OnceLock<RwLock<Environ>> = OnceLock::new();

/// The process-global discovered temp directory.
///
/// PORT NOTE: `state.?.tmp_dir_path`. Upstream computes it inside `global.init`
/// via `os/file.zig`'s `allocTmpDir(alloc, environ)`. `rustty-os` is ABOVE this
/// leaf crate (R4), so the discovery cannot be called from here; the
/// initializer publishes it with [`set_tmp_dir_path`] instead, and
/// [`tmp_dir_path`] falls back to the same rule when nothing published.
static TMP_DIR_PATH: OnceLock<Vec<u8>> = OnceLock::new();

fn environ_cell() -> &'static RwLock<Environ> {
    ENVIRON.get_or_init(|| RwLock::new(Environ::from_process()))
}

/// A read borrow of the process-global environment.
///
/// PORT NOTE: upstream's `environ()` returns `std.process.Environ` **by value**
/// because the Zig value is a fat pointer into the live block. rustty owns the
/// block, so the accessor hands out a guard instead; the guard derefs to
/// `Environ`, so `global::environ().get_posix(b"PATH")` reads the same as the
/// oracle's `global.environ().getPosix("PATH")`.
pub struct EnvironRef(RwLockReadGuard<'static, Environ>);

impl std::ops::Deref for EnvironRef {
    type Target = Environ;
    fn deref(&self) -> &Environ {
        &self.0
    }
}

/// Helper to return either the state's environment, or one from testing.
///
/// Asserts that the global state is initialized when not running as a test.
///
/// PORT NOTE: upstream asserts the global state is initialized when not running
/// as a test and returns `std.testing.environ` otherwise. See the module docs:
/// the rustty equivalent of the testing fallback is a lazy capture of the real
/// process environment, because `cfg(test)` is not transitive.
#[must_use]
pub fn environ() -> EnvironRef {
    EnvironRef(
        environ_cell()
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

/// Helper to create an environment map off of the state's environment, or one
/// from testing. The map is created off of the state allocator.
///
/// Asserts that the global state is initialized when not running as a test.
///
/// PORT NOTE: `pub fn environMap() !std.process.Environ.Map`.
#[must_use]
pub fn environ_map() -> EnvMap {
    environ().create_map()
}

/// Publish the captured environment block into the process-global state.
///
/// PORT NOTE: the `state = .{ .environ = ... }` assignment inside
/// `global.init`. Split out as a seam because rustty's `GlobalState` is generic
/// (see the module docs).
pub fn set_environ(new: Environ) {
    match ENVIRON.get() {
        Some(cell) => {
            *cell
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = new;
        }
        None => {
            // Racing initializers are fine: whoever loses simply overwrites.
            let _ = ENVIRON.set(RwLock::new(new.clone()));
            if let Some(cell) = ENVIRON.get() {
                *cell
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = new;
            }
        }
    }
}

/// Re-synchronizes the global Environ (both the higher-level and I/O versions)
/// from the process. No-op on Windows, asserts libc and an initialized global
/// state on everything else.
///
/// It is not valid to run this within any code that needs to be run through
/// tests. For any of these, re-factor the code to take an environment map
/// instead, where you can modify the environment as needed.
///
/// NOTE: Be cognizant of where you are calling this! While the only real
/// difference between the POSIX environment and higher-level Zig `PosixBlock`
/// struct is that the latter has a length versus the former's many-item
/// pointer state, there is no concurrency control on this function (or for
/// that matter, `environ` or `environMap`. Direct modification of the system
/// environment is becoming more discouraged in the standard library as well,
/// and this should be kept in mind when resorting to lower-level `setenv` or
/// `unsetenv` - as a rule, beyond initialization, favor
/// `std.process.Environ.Map` whenever possible.
///
/// PORT NOTE: the "higher-level and I/O versions" split is a Zig-ism —
/// upstream must re-point BOTH `state.environ` and `state.io_impl.environ`
/// because `std.Io.Threaded` caches its own copy. rustty has one owned copy
/// (see [`ENVIRON`]), so a single publish is the whole re-sync. Likewise
/// `assert(builtin.link_libc)` is vacuous here: Rust's `std` always links
/// libc, and `std::env::vars_os` is the same `environ` read.
//
// PORT NOTE: `pub fn syncEnviron() void`. Upstream re-reads `std.c.environ`
// directly; `std::env::vars_os` is the same read through Rust's std (which is
// itself `environ` on POSIX).
pub fn sync_environ() {
    #[cfg(windows)]
    {
        // PARITY: `.windows => {}` — no-op.
    }
    #[cfg(not(windows))]
    {
        set_environ(Environ::from_process());
    }
}

/// Publish the discovered temporary directory path.
///
/// PORT NOTE: `self.tmp_dir_path = try allocTmpDir(self.alloc, self.environ);`
/// inside `global.init`, inverted across the R4 layering (see [`TMP_DIR_PATH`]).
pub fn set_tmp_dir_path(path: Vec<u8>) {
    let _ = TMP_DIR_PATH.set(path);
}

/// Returns the temporary directory discovered from the global environment
/// (saves allocation of the temporary directory where you can get at global
/// state).
///
/// PORT NOTE: `pub fn tmpDirPath() []const u8`. Upstream asserts the global
/// state is initialized (`state.?.tmp_dir_path.?`); rustty falls back to the
/// same discovery rule `os/file.zig`'s `allocTmpDir` implements, because a
/// rustty library consumer (or a `#[test]`) may legitimately never have run
/// `init`. `rustty-os`'s `alloc_tmp_dir` and this fallback are asserted to
/// agree by `rustty-os`'s `alloc_tmp_dir_agrees_with_global_fallback` test.
#[must_use]
pub fn tmp_dir_path() -> &'static [u8] {
    TMP_DIR_PATH.get_or_init(default_tmp_dir_path)
}

/// The `allocTmpDir` discovery rule, duplicated at the leaf so the accessor has
/// no `rustty-os` edge. See [`tmp_dir_path`].
fn default_tmp_dir_path() -> Vec<u8> {
    #[cfg(windows)]
    {
        let dir = std::env::temp_dir();
        let mut bytes = os_bytes(dir.as_os_str()).to_vec();
        while bytes.last() == Some(&b'\\') {
            bytes.pop();
        }
        if bytes.is_empty() {
            return b"C:\\Windows\\Temp".to_vec();
        }
        bytes
    }
    #[cfg(not(windows))]
    {
        posix_tmp_dir_path(&environ())
    }
}

/// The POSIX arm of the `allocTmpDir` rule, on a given environment:
/// `environ.getPosix("TMPDIR") orelse environ.getPosix("TMP") orelse "/tmp"`,
/// trailing `/` trimmed.
///
/// PORT NOTE (parity S111-F1): `orelse` falls through only on a MISSING
/// variable, so a present-but-empty `TMPDIR` yields `""` and does not fall
/// through to `TMP`. (This used to filter out empty values, in step with the
/// same filter in `rustty-os`'s `allocTmpDir`.)
///
/// PORT NOTE: reference defect (R13), UD211. The reference's `trimEnd` turns
/// `TMPDIR=/` into `""`, which its own `TempDir.init` cannot open. A value
/// made only of `/` keeps the root and returns `/`. See `rustty-os`'s
/// `allocTmpDir` for the full note.
#[cfg(not(windows))]
fn posix_tmp_dir_path(env: &Environ) -> Vec<u8> {
    let Some(v) = env.get_posix(b"TMPDIR").or_else(|| env.get_posix(b"TMP")) else {
        return b"/tmp".to_vec();
    };
    let mut bytes = v.to_vec();
    while bytes.last() == Some(&b'/') {
        bytes.pop();
    }
    if bytes.is_empty() && !v.is_empty() {
        return b"/".to_vec();
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parity S111-F1: the fallback applies the reference rule to an empty
    /// value: present-but-empty is a value, not an absence. Same table as
    /// `rustty-os`'s `alloc_tmp_dir_takes_empty_and_non_utf8_values_verbatim`.
    #[test]
    #[cfg(not(windows))]
    fn posix_tmp_dir_path_keeps_empty_and_non_utf8_values() {
        let env =
            |entries: &[&[u8]]| Environ::from_entries(entries.iter().map(|e| e.to_vec()).collect());
        assert_eq!(posix_tmp_dir_path(&env(&[b"TMPDIR="])), b"");
        assert_eq!(posix_tmp_dir_path(&env(&[b"TMPDIR=", b"TMP=/x"])), b"");
        assert_eq!(posix_tmp_dir_path(&env(&[b"TMP=/x//"])), b"/x");
        assert_eq!(posix_tmp_dir_path(&env(&[b"TMPDIR=/a\xff/"])), b"/a\xff");
        assert_eq!(posix_tmp_dir_path(&env(&[])), b"/tmp");
    }

    /// R13 (UD211): a value made only of `/` keeps the root instead of
    /// becoming `""` as in the reference. Same table as `rustty-os`'s
    /// `alloc_tmp_dir_keeps_the_root_directory`.
    #[test]
    #[cfg(not(windows))]
    fn posix_tmp_dir_path_keeps_the_root_directory() {
        let env =
            |entries: &[&[u8]]| Environ::from_entries(entries.iter().map(|e| e.to_vec()).collect());
        assert_eq!(posix_tmp_dir_path(&env(&[b"TMPDIR=/"])), b"/");
        assert_eq!(posix_tmp_dir_path(&env(&[b"TMPDIR=///"])), b"/");
        assert_eq!(posix_tmp_dir_path(&env(&[b"TMP=/"])), b"/");
        assert_eq!(posix_tmp_dir_path(&env(&[b"TMPDIR="])), b"");
    }

    #[test]
    fn environ_get_posix_and_map_are_byte_typed() {
        // A NON-UTF-8 value must round-trip byte-exactly. `std::env::vars()`
        // (the shape this port replaces) panics on exactly this input.
        let e = Environ::from_entries(vec![
            b"PATH=/usr/bin".to_vec(),
            {
                let mut v = b"WEIRD=".to_vec();
                v.extend_from_slice(&[0xff, 0xfe, b'x']);
                v
            },
            b"EMPTY=".to_vec(),
            b"NOEQUALS".to_vec(),
        ]);

        // `getPosix` exists on POSIX only, as in the reference.
        #[cfg(not(windows))]
        {
            assert_eq!(e.get_posix(b"PATH"), Some(&b"/usr/bin"[..]));
            assert_eq!(e.get_posix(b"WEIRD"), Some(&[0xff_u8, 0xfe, b'x'][..]));
            assert_eq!(e.get_posix(b"EMPTY"), Some(&b""[..]));
            assert_eq!(e.get_posix(b"MISSING"), None);
        }
        assert_eq!(e.get_alloc(b"PATH"), Some(b"/usr/bin".to_vec()));
        assert_eq!(e.get_alloc(b"EMPTY"), Some(Vec::new()));
        assert_eq!(e.get_alloc(b"MISSING"), None);
        assert_eq!(e.get_alloc(b"WEIRD"), Some(vec![0xff, 0xfe, b'x']));

        let map = e.create_map();
        // The malformed `NOEQUALS` entry is skipped, like the oracle's parser.
        #[cfg(not(windows))]
        assert_eq!(map.len(), 3);
        // On Windows `putWindowsBlock` reads it as a name with an empty value
        // (a real Windows block always has the `=`; CreateProcess enforces it).
        #[cfg(windows)]
        {
            assert_eq!(map.len(), 4);
            assert_eq!(map.get(b"NOEQUALS").map(Vec::as_slice), Some(&b""[..]));
        }
        assert_eq!(
            map.get(&b"WEIRD"[..]).map(Vec::as_slice),
            Some(&[0xff_u8, 0xfe, b'x'][..])
        );
    }

    // `containsUnemptyConstant` (`e8525c0fd`) — present AND non-empty. This is
    // what `Config.probableCliEnvironment` asks of `TERM_PROGRAM`, so the
    // empty-value case is load-bearing: an exported-but-empty `TERM_PROGRAM`
    // must NOT read as a graphical terminal.
    #[test]
    fn contains_unempty_constant_requires_a_non_empty_value() {
        let e = Environ::from_entries(vec![
            b"TERM_PROGRAM=ghostty".to_vec(),
            b"EMPTY=".to_vec(),
            b"NOEQUALS".to_vec(),
        ]);

        assert!(e.contains_unempty_constant(b"TERM_PROGRAM"));
        assert!(!e.contains_unempty_constant(b"EMPTY"));
        assert!(!e.contains_unempty_constant(b"MISSING"));
        assert!(!e.contains_unempty_constant(b"NOEQUALS"));
        // A prefix of a real key must not match.
        assert!(!e.contains_unempty_constant(b"TERM"));
    }

    // POSIX only: `getPosix` does not exist on Windows, and there `createMap`
    // is `putWindowsBlock` (`putMove`: the last entry wins), which
    // `windows_create_map_puts_move_the_later_entry` covers.
    //
    // N381 (task W-5b): of two entries with the same name `getPosix` (and so
    // `containsConstant`/`containsUnemptyConstant`) takes the FIRST, and
    // `createMap` (and so `getAlloc`/`contains`/`containsUnempty`) the LAST, as
    // Zig 0.16's `putPosixBlock` `put`s every entry. CORRECTION: this test was
    // `environ_first_entry_wins` and asserted `create_map` kept the first.
    #[test]
    #[cfg(not(windows))]
    fn environ_get_posix_takes_the_first_duplicate_and_the_map_the_last() {
        let e = Environ::from_entries(vec![
            b"K=first".to_vec(),
            b"A=1".to_vec(),
            b"K=second".to_vec(),
        ]);
        assert_eq!(e.get_posix(b"K"), Some(&b"first"[..]));
        assert_eq!(e.get_alloc(b"K"), Some(b"second".to_vec()));
        let map = e.create_map();
        assert_eq!(map.get(b"K").map(Vec::as_slice), Some(&b"second"[..]));
        // `put` keeps the first entry's position.
        let keys: Vec<&[u8]> = map.keys().map(Vec::as_slice).collect();
        assert_eq!(keys, [&b"K"[..], &b"A"[..]]);
    }

    /// `containsUnemptyConstant` is NOT a bare presence check: upstream's two
    /// call sites (`Config.zig:5183` `TERM_PROGRAM`, `crash/sentry.zig:113`
    /// `XDG_CACHE_HOME`) both branch on "set AND non-empty", and a `KEY=` entry
    /// must read as ABSENT. `containsConstant` is the presence-only sibling and
    /// must disagree on exactly that entry.
    #[test]
    fn environ_contains_constant_vs_unempty_constant() {
        let e = Environ::from_entries(vec![
            b"SET=value".to_vec(),
            b"EMPTY=".to_vec(),
            // Non-UTF-8 value: the predicates are byte-typed, so this is
            // "present and non-empty" like any other.
            b"RAW=\xff\xfe".to_vec(),
        ]);

        assert!(e.contains_constant(b"SET"));
        assert!(e.contains_unempty_constant(b"SET"));

        // The load-bearing disagreement: present, but empty.
        assert!(e.contains_constant(b"EMPTY"));
        assert!(!e.contains_unempty_constant(b"EMPTY"));

        assert!(e.contains_constant(b"RAW"));
        assert!(e.contains_unempty_constant(b"RAW"));

        assert!(!e.contains_constant(b"MISSING"));
        assert!(!e.contains_unempty_constant(b"MISSING"));

        // Prefix/suffix keys must not match (`getPosix` splits on `=`).
        assert!(!e.contains_unempty_constant(b"SE"));
        assert!(!e.contains_unempty_constant(b"SETX"));
    }

    /// Both predicates read the SAME block `get_posix` does, so the
    /// first-entry-wins rule (`getenv(3)`) applies to them too: a later
    /// non-empty `K=` cannot resurrect an earlier empty one.
    /// N381: the map-based `containsUnempty` reads the LAST entry, so a later
    /// non-empty `K=later` makes it true where `containsUnemptyConstant` (the
    /// first entry: `getPosix` on POSIX, `getWindows` on Windows) is false.
    #[test]
    fn environ_contains_unempty_reads_the_last_entry() {
        let e = Environ::from_entries(vec![b"K=".to_vec(), b"K=later".to_vec()]);
        assert!(e.contains(b"K"));
        assert!(e.contains_unempty(b"K"));
        assert!(!e.contains_unempty_constant(b"K"));
        let e = Environ::from_entries(vec![b"K=first".to_vec(), b"K=".to_vec()]);
        assert!(!e.contains_unempty(b"K"));
        assert!(e.contains_unempty_constant(b"K"));
        assert!(!e.contains(b"MISSING"));
    }

    #[test]
    fn environ_contains_unempty_constant_respects_first_entry_wins() {
        let e = Environ::from_entries(vec![b"K=".to_vec(), b"K=later".to_vec()]);
        assert!(e.contains_constant(b"K"));
        assert!(!e.contains_unempty_constant(b"K"));
    }

    #[test]
    fn environ_from_process_matches_vars_os() {
        let e = Environ::from_process();
        // Every capture must be reachable through `get_posix` with the same
        // bytes `vars_os` reported.
        let mut checked = 0usize;
        for (k, v) in std::env::vars_os() {
            let kb = os_bytes(&k);
            if kb.contains(&b'=') {
                continue;
            }
            assert_eq!(
                e.get_alloc(kb),
                Some(os_bytes(&v).to_vec()),
                "key {k:?} did not round-trip"
            );
            checked += 1;
        }
        // R14 (G-SKIP LOOP, task S-15): an empty environment would run no
        // assertion above and report `ok`. `cargo test` always sets `CARGO_*`
        // variables, so an empty capture here is a broken harness, not a skip.
        assert!(
            checked > 0,
            "the process environment was empty, so nothing was round-tripped"
        );
    }

    #[test]
    // Miri runs with isolation on (the G6 lane deliberately does not pass
    // `-Zmiri-disable-isolation`), so the interpreted process has an EMPTY
    // environment and `PATH` is unset. The test would not merely fail there —
    // it would be VACUOUS if softened to "compare whatever is set", because
    // both sides would be empty. Runs natively in ci.sh's HARD
    // `cargo test --workspace` step, where the environment is real.
    // See .port/phase-2/MIRI_EXCEPTIONS.md, "rustty-global — real process state".
    #[cfg_attr(
        miri,
        ignore = "real process environment: Miri isolation gives an empty environ"
    )]
    fn global_environ_map_reflects_the_process() {
        // `environ_map()` must see the real process environment through the
        // lazy-capture fallback (the `builtin.is_test` arm's equivalent).
        let map = environ_map();
        let path = std::env::var_os("PATH").expect("PATH is always set under cargo test");
        assert_eq!(
            map.get(&b"PATH"[..]).map(Vec::as_slice),
            Some(os_bytes(&path))
        );
    }

    #[test]
    fn global_tmp_dir_path_is_absolute_and_unsuffixed() {
        let p = tmp_dir_path();
        assert!(!p.is_empty(), "tmp_dir_path() must never be empty");
        // The root directory keeps its only `/` (R13, UD211).
        assert!(
            p == b"/" || p.last() != Some(&b'/'),
            "trailing separator must be stripped"
        );
        #[cfg(not(windows))]
        assert_eq!(p.first(), Some(&b'/'), "POSIX tmp dir must be absolute");
    }

    #[cfg(windows)]
    fn first_entry(map: &EnvMap) -> Option<(&[u8], &[u8])> {
        map.iter().next().map(|(k, v)| (k.as_slice(), v.as_slice()))
    }

    /// N340: off Windows the map compares names byte for byte
    /// (`EnvNameHashContext`'s `else => std.array_hash_map.eqlString`), so `Path`
    /// and `PATH` are two variables, as in the reference.
    #[test]
    #[cfg(not(windows))]
    fn env_map_names_are_case_sensitive_off_windows() {
        let mut m = EnvMap::new();
        m.insert(b"Path".to_vec(), b"a".to_vec());
        assert_eq!(m.get(b"PATH"), None);
        m.insert(b"PATH".to_vec(), b"b".to_vec());
        assert_eq!(m.len(), 2);
        assert_eq!(m.get(b"Path").map(Vec::as_slice), Some(&b"a"[..]));
        assert_eq!(m.get(b"PATH").map(Vec::as_slice), Some(&b"b"[..]));

        let e = Environ::from_entries(vec![b"Path=x".to_vec()]);
        assert_eq!(e.get_alloc(b"PATH"), None);
        assert!(!e.contains_constant(b"PATH"));
        assert_eq!(e.create_map().get(b"PATH"), None);
        assert_eq!(e.get_alloc(b"Path"), Some(b"x".to_vec()));
    }

    /// N340: on Windows the map folds names with `toUpperWtf16` over their
    /// code points (`EnvNameHashContext`'s `.windows` arm), so `PATH` finds the
    /// `Path` Windows spells. The fold is Unicode upper-casing, not ASCII, and
    /// it does not apply above U+FFFF.
    #[test]
    #[cfg(windows)]
    fn windows_env_map_names_fold_case_like_the_reference() {
        let mut m = EnvMap::new();
        m.insert(b"Path".to_vec(), b"a".to_vec());
        assert_eq!(m.get(b"PATH").map(Vec::as_slice), Some(&b"a"[..]));
        assert_eq!(m.get(b"path").map(Vec::as_slice), Some(&b"a"[..]));
        assert!(m.contains_key(b"pAtH"));

        // `put`: the existing entry keeps its key spelling and takes the value.
        m.insert(b"PATH".to_vec(), b"b".to_vec());
        assert_eq!(m.len(), 1);
        assert_eq!(first_entry(&m), Some((&b"Path"[..], &b"b"[..])));
        // `putMove`: the key is replaced too.
        m.put_move(b"pATH".to_vec(), b"c".to_vec());
        assert_eq!(m.len(), 1);
        assert_eq!(first_entry(&m), Some((&b"pATH"[..], &b"c"[..])));
        assert_eq!(m.remove(b"PATH"), Some(b"c".to_vec()));
        assert!(m.is_empty());

        // Unicode: Λ/λ, and ɐ (U+0250, 2 bytes) / Ɐ (U+2C6F, 3 bytes).
        m.insert("λɐ".as_bytes().to_vec(), b"u".to_vec());
        assert_eq!(m.get("ΛⱯ".as_bytes()).map(Vec::as_slice), Some(&b"u"[..]));
        // No case folding at or above U+10000.
        m.insert("𐓏".as_bytes().to_vec(), b"v".to_vec());
        assert_eq!(m.get("𐓷".as_bytes()), None);
        assert_eq!(m.len(), 2);
    }

    /// `test "eqlIgnoreCaseWtf16/Wtf8"` (`std/os/windows.zig:3795-3799`), the
    /// WTF-8 half (the port has no WTF-16 comparison).
    #[test]
    #[cfg(windows)]
    fn windows_eql_ignore_case_wtf8() {
        assert!(eql_ignore_case_wtf8(
            "\x01 a B Λ ɐ".as_bytes(),
            "\x01 A b λ Ɐ".as_bytes()
        ));
        // does not do case-insensitive comparison for codepoints >= U+10000
        assert!(!eql_ignore_case_wtf8("𐓏".as_bytes(), "𐓷".as_bytes()));
    }

    /// N340: `getAlloc` (through `createMap`), `containsConstant` and
    /// `containsUnemptyConstant` (through `getWindows`) look names up
    /// case-insensitively on Windows; a name may start with `=`.
    #[test]
    #[cfg(windows)]
    fn windows_environ_lookups_fold_case() {
        let e = Environ::from_entries(vec![
            b"Path=C:\\x".to_vec(),
            b"=C:=C:\\w".to_vec(),
            b"SystemRoot=C:\\Windows".to_vec(),
            b"Empty=".to_vec(),
        ]);
        assert_eq!(e.get_alloc(b"PATH"), Some(b"C:\\x".to_vec()));
        assert!(e.contains_constant(b"SYSTEMROOT"));
        assert!(e.contains_unempty_constant(b"path"));
        assert!(e.contains_constant(b"EMPTY"));
        assert!(!e.contains_unempty_constant(b"EMPTY"));
        assert_eq!(
            e.create_map().get(b"=c:").map(Vec::as_slice),
            Some(&b"C:\\w"[..])
        );
        assert_eq!(e.get_windows(b"=c:"), Some(b"C:\\w".to_vec()));
        // '=' anywhere but the start makes this an invalid environment variable name.
        assert_eq!(e.get_windows(b"P=ATH"), None);
        assert_eq!(e.get_windows(b""), None);
    }

    /// On Windows `createMap` is `putWindowsBlock`, which `putMove`s each entry:
    /// of two names that differ only in case the later one wins, spelling
    /// included. `getWindows` returns the first match.
    #[test]
    #[cfg(windows)]
    fn windows_create_map_puts_move_the_later_entry() {
        let e = Environ::from_entries(vec![b"K=first".to_vec(), b"k=second".to_vec()]);
        let map = e.create_map();
        assert_eq!(map.len(), 1);
        assert_eq!(first_entry(&map), Some((&b"k"[..], &b"second"[..])));
        assert_eq!(e.get_windows(b"K"), Some(b"first".to_vec()));
    }

    /// N352: on Windows `from_process` is `.{ .use_global = true }`, which
    /// captures nothing (the reference reads the PEB on every lookup).
    #[test]
    #[cfg(windows)]
    fn windows_process_environ_is_the_global_block() {
        let e = Environ::from_process();
        assert!(e.use_global);
        assert!(e.entries.is_empty());
        assert_eq!(Environ::default(), Environ::from_entries(Vec::new()));
        assert!(!Environ::default().use_global);
        assert!(Environ::default().create_map().is_empty());
    }

    /// N384 (task W-5b): `EnvMap` iterates like the reference's `ArrayHashMap`:
    /// insertion order; `put` and `putMove` on an existing name keep its
    /// position; `orderedRemove` shifts the later entries; `swapRemove` moves
    /// the last entry into the hole. `createMap` keeps the block's order.
    #[test]
    fn env_map_keeps_the_array_hash_map_order() {
        fn keys(m: &EnvMap) -> Vec<&[u8]> {
            m.keys().map(Vec::as_slice).collect()
        }
        let mut m = EnvMap::new();
        for k in [&b"Z"[..], b"B", b"M", b"A", b"T"] {
            m.insert(k.to_vec(), b"1".to_vec());
        }
        assert_eq!(keys(&m), [&b"Z"[..], b"B", b"M", b"A", b"T"]);
        m.insert(b"B".to_vec(), b"2".to_vec());
        m.put_move(b"M".to_vec(), b"3".to_vec());
        assert_eq!(keys(&m), [&b"Z"[..], b"B", b"M", b"A", b"T"]);
        assert_eq!(m.get(b"M").map(Vec::as_slice), Some(&b"3"[..]));
        assert_eq!(m.remove(b"B"), Some(b"2".to_vec()));
        assert_eq!(keys(&m), [&b"Z"[..], b"M", b"A", b"T"]);
        assert_eq!(m.swap_remove(b"Z"), Some(b"1".to_vec()));
        assert_eq!(keys(&m), [&b"T"[..], b"M", b"A"]);
        assert_eq!(m.remove(b"missing"), None);

        let e = Environ::from_entries(vec![b"Q=1".to_vec(), b"C=2".to_vec(), b"X=3".to_vec()]);
        assert_eq!(keys(&e.create_map()), [&b"Q"[..], b"C", b"X"]);
    }
}
