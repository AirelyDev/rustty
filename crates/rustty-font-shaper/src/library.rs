//! A library represents the shared state that the underlying font
//! library implementation(s) require per-process.
//!
//! PORT: ghostty `src/font/library.zig`. That file is
//! `pub const Library = switch (options.backend)` over `FreetypeLibrary`
//! (freetype-active backends) and `NoopLibrary` (CoreText/Canvas). Both arms and
//! the backend `switch` are ported here so the `Library` type resolves to the
//! backend-appropriate implementation (macOS default `coretext` → `NoopLibrary`,
//! Linux default `fontconfig_freetype` → `FreetypeLibrary`), matching
//! `backend.zig`'s `Backend.default`. (The `freetype`/`harfbuzz`/`stb` FFI glue
//! this file leans on is superseded by the full `pkg/*` ports later.)

use crate::ft;

/// The library-wide mutex type.
// PORT NOTE: `*std.Thread.Mutex` → a heap `std::sync::Mutex<()>` reached through a
// raw pointer so the value-type `Library` can be freely copied (as in Zig) while
// all copies share the one underlying mutex + FreeType library.
pub type Mutex = std::sync::Mutex<()>;

/// Library implementation for the compile options.
// PORT: `library.zig`'s `pub const Library = switch (options.backend) { ...
// FreetypeLibrary ... NoopLibrary ... }`. Modeled with `cfg(target_os)` matching
// `Backend.default`: macOS → `NoopLibrary`, everything else → `FreetypeLibrary`.
#[cfg(target_os = "macos")]
pub type Library = NoopLibrary;
#[cfg(not(target_os = "macos"))]
pub type Library = FreetypeLibrary;

/// Library implementation for the freetype backend.
// PORT: `library.zig`'s `FreetypeLibrary`.
#[derive(Clone, Copy, Debug)]
pub struct FreetypeLibrary {
    /// The underlying FreeType library.
    pub lib: ft::Library,

    // PORT NOTE: the reference also holds `alloc: Allocator`, used only to
    // create/destroy the per-face and library mutexes. rustty uses the global
    // allocator (`Box`), so no allocator handle is threaded here.
    /// Mutex to be held any time the library is
    /// being used to create or destroy a face.
    pub mutex: *mut Mutex,
}

impl FreetypeLibrary {
    /// Initialize the library and its mutex.
    // PORT: `FreetypeLibrary.init`.
    pub fn init() -> Result<FreetypeLibrary, ft::FtError> {
        let lib = ft::Library::init()?;
        // errdefer lib.deinit();
        // PORT NOTE: `Box::new` cannot fail-return like `alloc.create`; OOM aborts.
        let mutex = Box::into_raw(Box::new(Mutex::new(())));
        Ok(FreetypeLibrary { lib, mutex })
    }

    /// Destroy the mutex and the underlying FreeType library.
    // PORT: `FreetypeLibrary.deinit`.
    pub fn deinit(&mut self) {
        // self.alloc.destroy(self.mutex);
        unsafe {
            drop(Box::from_raw(self.mutex));
        }
        self.lib.deinit();
    }
}

/// Library implementation for backends that don't need a "library" (CoreText,
/// Canvas).
// PORT: `library.zig`'s `NoopLibrary`.
//
//   pub const NoopLibrary = struct {
//       pub const InitError = error{};
//       pub fn init(alloc: Allocator) InitError!Library { _ = alloc; return Library{}; }
//       pub fn deinit(self: *Library) void { _ = self; }
//   };
//
// PORT NOTE: the reference `init` takes `alloc` (unused for the noop) and returns
// the empty error set `error{}`. rustty's `Library.init()` drops the `alloc`
// argument (the global allocator is used everywhere) and returns
// `Result<_, LibraryError>`; `LibraryError` is an uninhabited enum here so this
// arm can never actually error, matching the reference `error{}`.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopLibrary;

/// The (uninhabited) error set for `NoopLibrary.init`, mirroring the reference
/// `NoopLibrary.InitError = error{}`.
// PORT NOTE: on the freetype arm this is `ft::FtError`; the two arms share the
// `Library::init() -> Result<Library, _>` call shape.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub enum LibraryError {}

impl NoopLibrary {
    /// Initialize the (noop) library.
    // PORT: `NoopLibrary.init`.
    #[cfg(target_os = "macos")]
    pub fn init() -> Result<NoopLibrary, LibraryError> {
        Ok(NoopLibrary)
    }

    /// Destroy the (noop) library.
    // PORT: `NoopLibrary.deinit`.
    pub fn deinit(&mut self) {
        // _ = self;
    }
}
