//! rustty-oniguruma init
//!
//! PORTS: ghostty pkg/oniguruma/init.zig
//!
//! PORT STATUS: done. Process-level `onig_initialize`/`onig_end`.

use crate::errors::{self, Error};
use crate::types::Encoding;
use oniguruma_sys as c;
use std::os::raw::c_int;

/// Call once per process to initialize Oniguruma. This should be given
/// the encodings that the program will use.
// PORT NOTE: the reference `encs: []const *Encoding` is `&[*mut Encoding]`; the
// `@ptrCast(@alignCast(@constCast(encs.ptr)))` maps to an `as` cast of the slice
// pointer to `*mut OnigEncoding`.
pub fn init(encs: &[*mut Encoding]) -> Result<(), Error> {
    errors::convert_error(unsafe {
        c::onig_initialize(encs.as_ptr() as *mut c::OnigEncoding, encs.len() as c_int)
    })?;
    Ok(())
}

pub fn deinit() {
    unsafe {
        c::onig_end();
    }
}
