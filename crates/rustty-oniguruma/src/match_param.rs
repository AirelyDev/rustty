//! rustty-oniguruma match_param
//!
//! PORTS: ghostty pkg/oniguruma/match_param.zig
//!
//! PORT STATUS: done. `MatchParam` wraps a heap-allocated `OnigMatchParam` used
//! to bound a search's retry budget.

use crate::errors::{self, Error};
use oniguruma_sys as c;

pub struct MatchParam {
    pub value: *mut c::OnigMatchParam,
}

impl MatchParam {
    // PORT NOTE: `onig_new_match_param() orelse return Error.Memory` — a null
    // return is out-of-memory.
    pub fn init() -> Result<MatchParam, Error> {
        let value = unsafe { c::onig_new_match_param() };
        if value.is_null() {
            return Err(Error::Memory);
        }
        Ok(MatchParam { value })
    }

    pub fn deinit(&mut self) {
        unsafe { c::onig_free_match_param(self.value) }
    }

    pub fn set_retry_limit_in_search(&mut self, limit: usize) -> Result<(), Error> {
        errors::convert_error(unsafe {
            c::onig_set_retry_limit_in_search_of_match_param(
                self.value,
                limit as std::os::raw::c_ulong,
            )
        })?;
        Ok(())
    }
}
