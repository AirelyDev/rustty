//! rustty-datastruct
//!
//! PORTS: ghostty src/datastruct/ (array_list_collection, blocking_queue, cache_table, circ_buf,
//! comparison, intrusive_linked_list, lru, main, message_data, segmented_pool, split_tree).
//!
//! PORT STATUS: done
//!
//! The datastruct package contains data structures or anything closely
//! related to data structures.
//!
//! This crate is a leaf over rustty-util (fastmem, quirks). It has NO
//! dependency on the ghostty tree (R3) and is fully Miri-able (R6).

pub mod array_list_collection;
pub mod blocking_queue;
pub mod cache_table;
pub mod circ_buf;
pub mod comparison;
pub mod f16;
pub mod intrusive_linked_list;
pub mod lru;
mod main_mod;
pub mod message_data;
pub mod segmented_pool;
pub mod split_tree;

// Re-exports mirroring `datastruct/main.zig`'s public aliases.
//   pub const lru = @import("lru.zig");
//   pub const BlockingQueue = blocking_queue.BlockingQueue;
//   pub const CacheTable = cache_table.CacheTable;
//   pub const CircBuf = circ_buf.CircBuf;
//   pub const IntrusiveDoublyLinkedList = intrusive_linked_list.DoublyLinkedList;
//   pub const MessageData = @import("message_data.zig").MessageData;
//   pub const SegmentedPool = segmented_pool.SegmentedPool;
//   pub const SplitTree = split_tree.SplitTree;
pub use blocking_queue::BlockingQueue;
pub use cache_table::CacheTable;
pub use circ_buf::CircBuf;
pub use intrusive_linked_list::DoublyLinkedList as IntrusiveDoublyLinkedList;
pub use message_data::MessageData;
pub use segmented_pool::SegmentedPool;
pub use split_tree::SplitTree;
