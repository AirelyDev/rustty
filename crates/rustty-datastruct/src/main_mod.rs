//! The datastruct package contains data structures or anything closely
//! related to data structures.
//
// PORT NOTE: This is the port of `datastruct/main.zig`, the package root that
// re-exports the data structures. In Rust the crate root (`lib.rs`) performs the
// `pub use` re-exports; this module exists to carry `main.zig`'s module doc
// comment and its `refAllDecls` test verbatim. The Zig imports/`pub const`
// aliases map to the `pub use` lines in `lib.rs`:
//   - BlockingQueue               -> blocking_queue::BlockingQueue
//   - CacheTable                  -> cache_table::CacheTable
//   - CircBuf                     -> circ_buf::CircBuf
//   - IntrusiveDoublyLinkedList   -> intrusive_linked_list::DoublyLinkedList
//   - LimitedAllocator            -> limited_allocator::LimitedAllocator
//   - MessageData                 -> message_data::MessageData
//   - SplitTree                   -> split_tree::SplitTree
//   - WasmPagePool                -> wasm_page_pool::WasmPagePool

#[cfg(test)]
mod tests {
    // test {
    //     @import("std").testing.refAllDecls(@This());
    //     _ = @import("comparison.zig");
    // }
    //
    // `refAllDecls` forces analysis/compilation of every public decl and `_ =
    // @import("comparison.zig")` pulls in the comparison module's tests. In Rust
    // every module is compiled and every `#[test]` is collected automatically by
    // the test harness, so this is a no-op marker test that references the
    // re-exported decls + the comparison module to mirror the reference.
    #[test]
    fn test_anon_0() {
        // Reference the re-exported decls (the Rust analogue of refAllDecls).
        let _ = crate::comparison::deep_equal(&true, &true);
        let _ = crate::circ_buf::Direction::Forward;
        // comparison's own tests are compiled and run by the harness, mirroring
        // `_ = @import("comparison.zig")`.
    }
}
