//! rustty-lib
//!
//! PORTS: ghostty src/lib/ (allocator, allocator/convenience, enum, main, string, struct,
//! target, types, union) — allocator utilities.
//!
//! PORT STATUS: done
//!
//! PORT NOTE: This crate ports ghostty `src/lib/` — the allocator vtable/ABI
//! (`allocator`, `allocator/convenience`) plus the comptime ABI helper type
//! factories (`enum`, `struct`, `union`, `target`, `types`, `string`). The
//! comptime type factories are modeled as runtime descriptors that reproduce
//! the SAME layout/value algorithms the reference tests observe.
//!
//! `lib/main.zig` is ported here as the crate root (the module re-export hub).

#![allow(clippy::manual_div_ceil)]

pub mod allocator;
pub mod r#enum;
pub mod string;
pub mod r#struct;
pub mod target;
pub mod types;
pub mod r#union;

// ----- lib/main.zig: re-export hub -----

/// PORT NOTE: `pub const allocator = @import("allocator.zig");` is the
/// `allocator` module above. The remaining `main.zig` re-exports map onto the
/// items below.
pub use crate::r#enum::{check_ghostty_henum, enum_type as Enum};
pub use crate::r#struct::{sized_field_fits as struct_sized_field_fits, struct_type as Struct};
pub use crate::r#union::TaggedUnion;
pub use crate::string::cut_prefix;
pub use crate::target::Target;
pub use crate::types::{Buffer, String};

#[cfg(test)]
mod main_tests {
    #[test]
    fn test_anon_0() {
        // PORT NOTE: ghostty `lib/main.zig` `test { std.testing.refAllDecls(@This()); }`
        // — forces every public decl to be referenced/compiled. In Rust the
        // crate's items are compiled by the test build; we reference the
        // re-exports to mirror `refAllDecls`.
        #[allow(unused_imports)]
        use super::{
            cut_prefix, struct_sized_field_fits, Buffer, Enum, String, Struct, TaggedUnion, Target,
        };
        let _ = Target::C;
        let _ = Target::Zig;
    }
}

pub mod allocator_convenience {
    //! PORT NOTE: re-export of `allocator/convenience.zig` under the crate-root
    //! path (`allocator.convenience` in the reference).
    pub use crate::allocator::convenience::*;
}
