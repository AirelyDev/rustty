//! A library represents the shared state that the underlying font
//! library implementation(s) require per-process.
//!
//! PORT: ghostty `src/font/Library.zig`. That file is a `switch(options.backend)`
//! over `FreetypeLibrary` (freetype-active backends) and `NoopLibrary` (CoreText/
//! Canvas). Since this tier is porting the FreeType face, `Library` here is the
//! `FreetypeLibrary` arm; the `NoopLibrary` arm + the backend `switch` land with
//! the CoreText face. (The full `Library.zig` port, with its selector, supersedes
//! this glue.)

use crate::ft;

/// The library-wide mutex type.
// PORT NOTE: `*std.Thread.Mutex` → a heap `std::sync::Mutex<()>` reached through a
// raw pointer so the value-type `Library` can be freely copied (as in Zig) while
// all copies share the one underlying mutex + FreeType library.
pub type Mutex = std::sync::Mutex<()>;

/// Library implementation for the freetype backend.
// PORT: `Library.zig`'s `FreetypeLibrary`.
#[derive(Clone, Copy, Debug)]
pub struct Library {
    /// The underlying FreeType library.
    pub lib: ft::Library,

    // PORT NOTE: the reference also holds `alloc: Allocator`, used only to
    // create/destroy the per-face and library mutexes. rustty uses the global
    // allocator (`Box`), so no allocator handle is threaded here.
    /// Mutex to be held any time the library is
    /// being used to create or destroy a face.
    pub mutex: *mut Mutex,
}

impl Library {
    /// Initialize the library and its mutex.
    // PORT: `FreetypeLibrary.init`.
    pub fn init() -> Result<Library, ft::FtError> {
        let lib = ft::Library::init()?;
        // errdefer lib.deinit();
        // PORT NOTE: `Box::new` cannot fail-return like `alloc.create`; OOM aborts.
        let mutex = Box::into_raw(Box::new(Mutex::new(())));
        Ok(Library { lib, mutex })
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
