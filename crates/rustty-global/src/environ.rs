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

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::sync::{OnceLock, RwLock, RwLockReadGuard};

/// PORT NOTE: `std.process.Environ.Map` — the owned key/value map built off the
/// captured block. Byte-typed for the same reason `Environ` is.
///
/// Upstream's `Map` is an `ArrayHashMap`, so it has `swapRemove`/`orderedRemove`
/// (`shell_integration.zig` uses `swapRemove` for `GHOSTTY_BASH_ENV` and
/// `orderedRemove` for `VTE_VERSION`/`LD_LIBRARY_PATH`). The distinction is
/// **not observable**: the map is only ever iterated to build `envp`, where
/// order is unspecified. rustty's map is a plain `HashMap`, which has a single
/// `remove`, so there is no ordered/unordered choice to mirror — recorded here
/// rather than silently dropped.
pub type EnvMap = HashMap<Vec<u8>, Vec<u8>>;

/// PORT NOTE: `std.process.Environ` — the captured `KEY=VALUE` environment
/// block. Upstream borrows into the live process block; rustty owns a copy,
/// because Rust has no way to hand out a `'static` borrow of `**environ` that
/// stays sound across a `setenv`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environ {
    /// The `KEY=VALUE` entries, in block order.
    entries: Vec<Vec<u8>>,
}

impl Environ {
    /// Capture the current process environment.
    ///
    /// PORT NOTE: this is `std.process.Init.Minimal.environ` (the `main` arm of
    /// upstream's `InitOpts`) — the environment as the process was started
    /// with, plus any in-process mutation up to this point. Uses `vars_os`, NOT
    /// `vars`: `std::env::vars` **panics** on a non-UTF-8 variable and POSIX
    /// environment variables are arbitrary byte strings.
    #[must_use]
    pub fn from_process() -> Self {
        let mut entries = Vec::new();
        for (k, v) in std::env::vars_os() {
            let mut entry = os_bytes(&k).to_vec();
            entry.push(b'=');
            entry.extend_from_slice(os_bytes(&v));
            entries.push(entry);
        }
        Self { entries }
    }

    /// Build an `Environ` from raw `KEY=VALUE` entries.
    ///
    /// PORT NOTE: upstream's `.c` arm takes the block straight from the C
    /// caller (`global.init(.{ .c = .{ .environ = ... } })`).
    #[must_use]
    pub fn from_entries(entries: Vec<Vec<u8>>) -> Self {
        Self { entries }
    }

    /// The raw `KEY=VALUE` entries in block order.
    #[must_use]
    pub fn entries(&self) -> &[Vec<u8>] {
        &self.entries
    }

    /// PORT NOTE: `pub fn getPosix(self: Environ, key: []const u8) ?[:0]const u8`
    /// — a borrow into the block, no allocation. Returns the FIRST match, which
    /// is what `getenv(3)` does.
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

    /// PORT NOTE: `pub fn getAlloc(self: Environ, alloc, key) ![]u8` — the
    /// portable (Windows-safe) copying form. Upstream returns
    /// `error.EnvironmentVariableMissing` when absent; Rust's `Option` carries
    /// that.
    #[must_use]
    pub fn get_alloc(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.get_posix(key).map(<[u8]>::to_vec)
    }

    /// PORT NOTE: `pub fn createMap(self: Environ, alloc) !Map`.
    #[must_use]
    pub fn create_map(&self) -> EnvMap {
        let mut map = EnvMap::with_capacity(self.entries.len());
        for entry in &self.entries {
            let Some(eq) = entry.iter().position(|&b| b == b'=') else {
                continue;
            };
            // First entry wins, matching `getPosix`/`getenv(3)`.
            map.entry(entry[..eq].to_vec())
                .or_insert_with(|| entry[eq + 1..].to_vec());
        }
        map
    }
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

/// Helper to return the state's environment.
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

/// Helper to create an environment map off of the state's environment.
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

/// Re-synchronizes the global Environ from the process. No-op on Windows.
///
/// It is not valid to run this within any code that needs to be run through
/// tests. For any of these, re-factor the code to take an environment map
/// instead, where you can modify the environment as needed.
///
/// NOTE: Be cognizant of where you are calling this! There is no concurrency
/// control on this function (or for that matter, `environ` or `environMap`).
/// Direct modification of the system environment is becoming more discouraged,
/// and this should be kept in mind when resorting to lower-level `setenv` or
/// `unsetenv` — as a rule, beyond initialization, favor an environment map
/// whenever possible.
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
        let env = environ();
        let Some(v) = env
            .get_posix(b"TMPDIR")
            .or_else(|| env.get_posix(b"TMP"))
            .filter(|v| !v.is_empty())
        else {
            return b"/tmp".to_vec();
        };
        let mut bytes = v.to_vec();
        while bytes.last() == Some(&b'/') {
            bytes.pop();
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(e.get_posix(b"PATH"), Some(&b"/usr/bin"[..]));
        assert_eq!(e.get_posix(b"WEIRD"), Some(&[0xff_u8, 0xfe, b'x'][..]));
        assert_eq!(e.get_posix(b"EMPTY"), Some(&b""[..]));
        assert_eq!(e.get_posix(b"MISSING"), None);
        assert_eq!(e.get_alloc(b"WEIRD"), Some(vec![0xff, 0xfe, b'x']));

        let map = e.create_map();
        // The malformed `NOEQUALS` entry is skipped, like the oracle's parser.
        assert_eq!(map.len(), 3);
        assert_eq!(
            map.get(&b"WEIRD"[..]).map(Vec::as_slice),
            Some(&[0xff_u8, 0xfe, b'x'][..])
        );
    }

    #[test]
    fn environ_first_entry_wins() {
        let e = Environ::from_entries(vec![b"K=first".to_vec(), b"K=second".to_vec()]);
        assert_eq!(e.get_posix(b"K"), Some(&b"first"[..]));
        assert_eq!(
            e.create_map().get(&b"K"[..]).map(Vec::as_slice),
            Some(&b"first"[..])
        );
    }

    #[test]
    fn environ_from_process_matches_vars_os() {
        let e = Environ::from_process();
        // Every capture must be reachable through `get_posix` with the same
        // bytes `vars_os` reported.
        for (k, v) in std::env::vars_os() {
            let kb = os_bytes(&k);
            if kb.contains(&b'=') {
                continue;
            }
            assert_eq!(
                e.get_posix(kb).map(<[u8]>::to_vec),
                Some(os_bytes(&v).to_vec()),
                "key {k:?} did not round-trip"
            );
        }
    }

    #[test]
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
        assert_ne!(p.last(), Some(&b'/'), "trailing separator must be stripped");
        #[cfg(not(windows))]
        assert_eq!(p.first(), Some(&b'/'), "POSIX tmp dir must be absolute");
    }
}
