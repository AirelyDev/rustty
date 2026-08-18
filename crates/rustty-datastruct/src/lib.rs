//! rustty-datastruct
//!
//! PORTS: ghostty src/datastruct/ (blocking_queue, cache_table, circ_buf,
//! comparison, intrusive_linked_list, limited_allocator, main, message_data,
//! split_tree, wasm_page_pool).
//!
//! PORT STATUS: done
//!
//! The datastruct package contains data structures or anything closely
//! related to data structures.
//!
//! This crate is a leaf over rustty-util (fastmem, quirks). It has NO
//! dependency on the ghostty tree (R3) and is fully Miri-able (R6).

pub mod blocking_queue;
pub mod cache_table;
pub mod circ_buf;
pub mod comparison;
pub mod f16;
pub mod intrusive_linked_list;
pub mod limited_allocator;
mod main_mod;
pub mod message_data;
pub mod split_tree;
pub mod wasm_page_pool;

// Re-exports mirroring `datastruct/main.zig`'s public aliases.
//   pub const BlockingQueue = blocking_queue.BlockingQueue;
//   pub const CacheTable = cache_table.CacheTable;
//   pub const CircBuf = circ_buf.CircBuf;
//   pub const IntrusiveDoublyLinkedList = intrusive_linked_list.DoublyLinkedList;
//   pub const LimitedAllocator = @import("limited_allocator.zig").LimitedAllocator;
//   pub const MessageData = @import("message_data.zig").MessageData;
//   pub const SplitTree = split_tree.SplitTree;
//   pub const WasmPagePool = @import("wasm_page_pool.zig").WasmPagePool;
pub use blocking_queue::BlockingQueue;
pub use cache_table::CacheTable;
pub use circ_buf::CircBuf;
pub use intrusive_linked_list::DoublyLinkedList as IntrusiveDoublyLinkedList;
pub use limited_allocator::LimitedAllocator;
pub use message_data::MessageData;
pub use split_tree::SplitTree;
pub use wasm_page_pool::WasmPagePool;
