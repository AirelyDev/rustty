//! rustty-apprt::gtk::ext::slice
//!
//! PORTS: ghostty `src/apprt/gtk/ext/slice.zig` (111 loc, 3 tests @ `:67,:80,:91`).
//!
//! A boxed `GType` holding a list of strings (`GhosttyStringList`). The oracle
//! backs the storage with an `ArenaAllocator` + a slice of sentinel-terminated
//! `[:0]const u8` and defines copy/free callbacks by hand
//! (`gobject.ext.defineBoxed`, `slice.zig:44-64`). Under Rust ownership the arena
//! collapses to an owned `Vec<String>`: `Clone` performs the deep copy the
//! oracle's `copy` callback does (re-`create` from the source strings,
//! `slice.zig:49-55`) and `Drop` performs the arena `deinit` the `free` callback
//! does (`slice.zig:57-61`). `#[derive(glib::Boxed)]` wires those to the
//! `GBoxed` `copy`/`free` slots via `Box::clone`/`Box::drop`
//! (glib-0.22 `subclass/boxed.rs:50-63`), giving byte-for-byte the oracle's
//! deep-copy boxed semantics.
//!
//! With the arena gone, the oracle's `allocator` accessor (`slice.zig:39-42`)
//! has nothing to return and is not ported. Its reference doc:
//!
//! Returns the general-purpose allocator used by this StringList.
//!
//! Sentinel note: the oracle stores `[:0]const u8` because these feed C APIs.
//! rustty holds owned `String`; a future consumer that hands one to a C boundary
//! re-establishes the NUL there (`CString`). No such consumer exists in this
//! task, so the storage stays plain `String`.

/// A boxed type that holds a list of string slices.
///
/// PORTS: `slice.zig:8` `pub const StringList`, registered as `"GhosttyStringList"` (`slice.zig:47`).
#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "GhosttyStringList", nullable)]
pub struct StringList {
    strings: Vec<String>,
}

impl StringList {
    /// PORTS: `slice.zig:12` `create`. Deep-copies each input string into owned
    /// storage (the oracle's arena `dupeZ`, `slice.zig:19-20`).
    #[must_use]
    pub fn new(strings: &[&str]) -> Self {
        Self {
            strings: strings.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    /// The stored strings. PORTS: the oracle's `strings: []const [:0]const u8`
    /// field (`slice.zig:10`).
    #[must_use]
    pub fn strings(&self) -> &[String] {
        &self.strings
    }

    /// Number of strings. PORTS: the oracle's `list.strings.len` reads
    /// (`slice.zig:75,88,105`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Whether the list is empty (clippy pairs this with [`StringList::len`]).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtk::ext;

    /// PORTS: `slice.zig:67` `test "StringList create and destroy"`. (`destroy`
    /// is the oracle's manual arena teardown; in rustty it is `Drop`, exercised
    /// implicitly at scope end.)
    #[test]
    fn string_list_create_and_destroy() {
        let list = StringList::new(&["hello", "world"]);
        assert_eq!(list.len(), 2);
        assert_eq!(list.strings()[0], "hello");
        assert_eq!(list.strings()[1], "world");
    }

    /// PORTS: `slice.zig:80` `test "StringList create empty list"`.
    #[test]
    fn string_list_create_empty() {
        let list = StringList::new(&[]);
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
    }

    /// PORTS: `slice.zig:91` `test "StringList boxedCopy and boxedFree"`.
    ///
    /// Drives the registered `GBoxed` copy/free through [`ext::boxed_copy`] /
    /// [`ext::boxed_free`] (the oracle uses `gobject.boxedCopy`/`boxedFree`
    /// directly, `slice.zig:99,103`), and asserts the copy is DEEP: the copied
    /// storage lives at a different address than the original
    /// (`slice.zig:110` `original.strings.ptr != copied.strings.ptr`).
    ///
    /// `g_boxed_copy` reads through the `&original` pointer and clones into a
    /// fresh heap box; `original` stays owned by this scope, and only the copy is
    /// freed via `boxed_free` (mirroring the oracle's `defer original.destroy()`
    /// + `defer gobject.boxedFree(copied)`).
    #[test]
    fn string_list_boxed_copy_and_free() {
        let original = StringList::new(&["foo", "bar", "baz"]);

        let copied_ptr = ext::boxed_copy(std::ptr::from_ref(&original));
        assert!(!copied_ptr.is_null());

        // SAFETY: g_boxed_copy returned a live, owned `StringList` box.
        let copied: &StringList = unsafe { &*copied_ptr };
        assert_eq!(copied.len(), 3);
        assert_eq!(copied.strings()[0], "foo");
        assert_eq!(copied.strings()[1], "bar");
        assert_eq!(copied.strings()[2], "baz");

        // deep copy: different backing allocation than the original
        assert_ne!(
            original.strings().as_ptr(),
            copied.strings().as_ptr(),
            "boxedCopy must deep-copy the string storage"
        );

        ext::boxed_free(copied_ptr);
    }
}
