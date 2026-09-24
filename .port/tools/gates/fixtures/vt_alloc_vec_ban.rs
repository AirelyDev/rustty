// Fixture for `check_vt_alloc_vec_ban.sh --self-test` (gate G-VTALLOC).
//
// Compiled as the lib of a throwaway crate outside the workspace, under each
// vt crate's clippy.toml. The self-test requires EXACTLY the two lines marked
// `BANNED` to be reported as `disallowed_methods`, and nothing else: std's
// `Vec` methods of the same names and the allocator_api2 methods the
// replacement helpers use must stay allowed.

use allocator_api2::alloc::Global;
use allocator_api2::vec::Vec as AVec;

pub fn banned(v: &mut AVec<u8, Global>, src: &[u8]) {
    v.extend_from_slice(src); // BANNED: extend_from_slice
    v.resize(16, 0); // BANNED: resize
}

// The recorded residual (clippy.toml "NOT BANNABLE"): allocator_api2's
// `Extend` impls take the same per-element loop, but clippy cannot name a
// trait-impl method, so these are NOT flagged. The exact-set check above keeps
// this truthful: if a clippy release starts flagging them, the self-test goes
// red and the ban (and this comment) must be extended.
pub fn residual(v: &mut AVec<u8, Global>, src: &[u8]) {
    v.extend(src.iter().copied());
    v.extend(src);
}

pub fn allowed(v: &mut AVec<u8, Global>, s: &mut Vec<u8>, src: &[u8]) {
    s.extend_from_slice(src);
    s.resize(16, 0);
    v.reserve(src.len());
    v.truncate(1);
    let _ = v.try_reserve(4);
    v.push(1);
    let _ = v.spare_capacity_mut();
}
