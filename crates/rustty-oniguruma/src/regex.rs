//! rustty-oniguruma regex
//!
//! PORTS: ghostty pkg/oniguruma/regex.zig
//!
//! PORT STATUS: done. `Regex` is the compiled-pattern handle plus the search
//! surface (`search`/`searchWithParam`/`searchAdvanced*`).

use crate::errors::{self, Error, ErrorInfo};
use crate::match_param::MatchParam;
use crate::region::Region;
use crate::types::{Encoding, RegexOptions, SearchOptions, Syntax};
use oniguruma_sys as c;

pub struct Regex {
    pub value: c::OnigRegex,
}

impl Regex {
    // PORT NOTE: `onig_new` compiles the pattern; the reference passes
    // `pattern.ptr` and `pattern.ptr + pattern.len` as the `[begin, end)` bounds.
    // `err` is optional (`?*ErrorInfo`) -> `Option<&mut ErrorInfo>`.
    pub fn init(
        pattern: &[u8],
        options: RegexOptions,
        enc: *mut Encoding,
        syntax: *mut Syntax,
        err: Option<&mut ErrorInfo>,
    ) -> Result<Regex, Error> {
        let mut value: c::OnigRegex = std::ptr::null_mut();
        let err_ptr = match err {
            Some(e) => e as *mut ErrorInfo as *mut c::OnigErrorInfo,
            None => std::ptr::null_mut(),
        };
        errors::convert_error(unsafe {
            c::onig_new(
                &mut value,
                pattern.as_ptr(),
                pattern.as_ptr().add(pattern.len()),
                options.int(),
                enc as c::OnigEncoding,
                syntax as *mut c::OnigSyntaxType,
                err_ptr,
            )
        })?;
        Ok(Regex { value })
    }

    pub fn deinit(&mut self) {
        unsafe { c::onig_free(self.value) }
    }

    /// Search an entire string for matches. This always returns a region
    /// which may heap allocate (C allocator).
    pub fn search(&mut self, str: &[u8], options: SearchOptions) -> Result<Region, Error> {
        // PORT NOTE: the reference `search` delegates to `searchWithParam(…, null)`.
        self.search_impl(str, options, None)
    }

    /// Search an entire string for matches. This always returns a region
    /// which may heap allocate (C allocator).
    // PORT NOTE: the frozen `oni::Region search_with_param(bytes, SearchOptions,
    // &mut MatchParam)` takes a REQUIRED match param (that is exactly how vt's
    // `string_map.rs` calls it); the reference's optional-param form is the
    // private `search_impl`/`search_advanced_with_param` below.
    pub fn search_with_param(
        &mut self,
        str: &[u8],
        options: SearchOptions,
        match_param: &mut MatchParam,
    ) -> Result<Region, Error> {
        self.search_impl(str, options, Some(match_param))
    }

    // PORT NOTE: the reference `searchWithParam` body: build a zeroed `Region`,
    // `errdefer region.deinit()` (onig may allocate even on a failed match, so we
    // free it on error), run `searchAdvancedWithParam`, and return the region.
    fn search_impl(
        &mut self,
        str: &[u8],
        options: SearchOptions,
        match_param: Option<&mut MatchParam>,
    ) -> Result<Region, Error> {
        let mut region = Region::default();

        // As part of the searchAdvanced API call below, onig may allocate
        // memory even if we fail to match. We need to deinit if there are
        // any errors to free that memory.
        match self.search_advanced_with_param(str, 0, str.len(), &mut region, options, match_param)
        {
            Ok(_) => Ok(region),
            Err(err) => {
                region.deinit();
                Err(err)
            }
        }
    }

    /// onig_search directly
    pub fn search_advanced(
        &mut self,
        str: &[u8],
        start: usize,
        end: usize,
        region: &mut Region,
        options: SearchOptions,
    ) -> Result<usize, Error> {
        self.search_advanced_with_param(str, start, end, region, options, None)
    }

    /// onig_search_with_param directly
    pub fn search_advanced_with_param(
        &mut self,
        str: &[u8],
        start: usize,
        end: usize,
        region: &mut Region,
        options: SearchOptions,
        match_param: Option<&mut MatchParam>,
    ) -> Result<usize, Error> {
        let str_ptr = str.as_ptr();
        let region_ptr = region as *mut Region as *mut c::OnigRegion;
        let pos = errors::convert_error(unsafe {
            match match_param {
                Some(param) => c::onig_search_with_param(
                    self.value,
                    str_ptr,
                    str_ptr.add(str.len()),
                    str_ptr.add(start),
                    str_ptr.add(end),
                    region_ptr,
                    options.int(),
                    param.value,
                ),
                None => c::onig_search(
                    self.value,
                    str_ptr,
                    str_ptr.add(str.len()),
                    str_ptr.add(start),
                    str_ptr.add(end),
                    region_ptr,
                    options.int(),
                ),
            }
        })?;

        Ok(pos as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::ensure_init;

    // PORT NOTE: faithful port of the reference `test {}` in regex.zig — compile
    // `foo`, match "hello foo bar" (1 region), mismatch "hello", then repeat with
    // a bounded `MatchParam`.
    #[test]
    fn search_and_search_with_param() {
        ensure_init().unwrap();
        let mut re = Regex::init(
            b"foo",
            RegexOptions::default(),
            Encoding::utf8(),
            Syntax::default(),
            None,
        )
        .unwrap();
        // defer re.deinit();

        let mut reg = re
            .search(b"hello foo bar", SearchOptions::default())
            .unwrap();
        // defer reg.deinit();
        assert_eq!(1usize, reg.count());
        reg.deinit();

        // PORT NOTE: `expectError(Error.Mismatch, …)` — a non-matching search is
        // the `Mismatch` error. `matches!` avoids requiring `Region: Debug` on
        // the FFI-layout struct just to format the `Ok` arm.
        assert!(matches!(
            re.search(b"hello", SearchOptions::default()),
            Err(Error::Mismatch)
        ));

        let mut match_param = MatchParam::init().unwrap();
        // defer match_param.deinit();
        match_param.set_retry_limit_in_search(1000).unwrap();

        let mut reg_param = re
            .search_with_param(b"hello foo bar", SearchOptions::default(), &mut match_param)
            .unwrap();
        // defer reg_param.deinit();
        assert_eq!(1usize, reg_param.count());
        reg_param.deinit();
        match_param.deinit();

        re.deinit();
    }
}
