//! rustty-os::args
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/args.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT: the Zig reference imports `std`, `builtin`, `Allocator = std.mem.Allocator`,
// `objc`, and `macos`. In Rust we use the standard library plus, on macOS,
// `objc2`/`objc2-foundation` (R3: bindings to the SAME Objective-C
// runtime/Foundation framework the reference uses) for the `NSProcessInfo`
// argument iterator.
#![allow(non_snake_case)]

/// Returns an iterator over the command line arguments. This may or may
/// not allocate depending on the platform.
///
/// For Zig-aware readers: this is the same as std.process.argsWithAllocator
/// but handles macOS using NSProcessInfo instead of libc argc/argv.
pub fn iterator() -> Result<ArgIterator, InitError> {
    //if (true) return try std.process.argsWithAllocator(allocator);
    ArgIterator::initWithAllocator()
}

// PORT: `pub const ArgIterator = switch (builtin.os.tag) { .macos =>
// IteratorMacOS, else => std.process.ArgIterator };`. Duck-typed to
// std.process.ArgIterator.
//
// On macOS we use `IteratorMacOS`; on every other platform we wrap
// `std::env::args` (the std iterator the reference falls back to). The init
// error type is `InitError` on both.

#[cfg(target_os = "macos")]
pub type ArgIterator = IteratorMacOS;

/// PORT: `error{...}` for the macOS iterator init; the reference is
/// `Allocator.Error`. Rust's allocation failure is not a recoverable error, so
/// the parity-preserving init error type is the empty error set; we model it as
/// a unit error so the public `Result` surface matches the reference.
#[derive(Debug, PartialEq, Eq)]
pub enum InitError {}

#[cfg(not(target_os = "macos"))]
pub type ArgIterator = IteratorStd;

/// PORT: the `else => std.process.ArgIterator` branch. A thin wrapper over
/// `std::env::args` that exposes the same `next`/`skip`/`deinit` surface as the
/// macOS iterator (duck-typed to std.process.ArgIterator).
#[cfg(not(target_os = "macos"))]
pub struct IteratorStd {
    inner: std::env::Args,
}

#[cfg(not(target_os = "macos"))]
impl IteratorStd {
    pub fn initWithAllocator() -> Result<IteratorStd, InitError> {
        Ok(IteratorStd {
            inner: std::env::args(),
        })
    }

    pub fn deinit(&mut self) {}

    // PARITY: named `next` (duck-typed to std.process.ArgIterator).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<String> {
        self.inner.next()
    }

    pub fn skip(&mut self) -> bool {
        self.inner.next().is_some()
    }
}

/// This is an ArgIterator (duck-typed for std.process.ArgIterator) for
/// NSApplicationMain-based applications on macOS. It uses NSProcessInfo to
/// get the command line arguments since libc argc/argv pointers are not
/// valid.
///
/// I believe this should work for all macOS applications even if
/// NSApplicationMain is not used, but I haven't tested that so I'm not
/// sure. If/when librustty is ever used outside of NSApplicationMain
/// then we can revisit this.
#[cfg(target_os = "macos")]
pub struct IteratorMacOS {
    index: usize,
    count: usize,
    // PORT: the reference holds an explicit sentinel-terminated heap `buf` that it
    // reuses across `next` calls (precomputed to the max arg length) and the
    // `args` NSArray object. In Rust we hold the retained `NSArray<NSString>`; the
    // per-call byte buffer below is reproduced as `buf` to mirror the reference's
    // `getCString:` into a precomputed buffer (no re-allocation per call).
    buf: Vec<u8>,
    args: objc2::rc::Retained<objc2_foundation::NSArray<objc2_foundation::NSString>>,
}

#[cfg(target_os = "macos")]
impl IteratorMacOS {
    pub fn initWithAllocator() -> Result<IteratorMacOS, InitError> {
        use objc2_foundation::NSProcessInfo;

        let info = NSProcessInfo::processInfo();
        let args = info.arguments();

        // Determine our maximum length so we can allocate the buffer to
        // fit all values.
        let mut max: usize = 0;
        let count: usize = args.count();
        for i in 0..count {
            let nsstr = args.objectAtIndex(i);

            let maxlen: usize = nsstr.maximumLengthOfBytesUsingEncoding(
                // PORT: reference passes `@as(c_ulong, 4)` (NSUTF8StringEncoding).
                4 as objc2_foundation::NSStringEncoding,
            );

            max = max.max(maxlen);
        }

        // Allocate our buffer. We add 1 for the null terminator.
        let buf = vec![0u8; max + 1];

        Ok(IteratorMacOS {
            index: 0,
            count,
            buf,
            args,
        })
    }

    pub fn deinit(&mut self) {
        // PORT: the reference frees its `buf`; in Rust the `Vec` is dropped here.

        // Note: we don't release self.args because it is a pointer copy
        // not a retained object.
    }

    // PARITY: the reference method is named `next` (duck-typed to
    // std.process.ArgIterator); kept verbatim though it shadows Iterator::next.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<String> {
        if self.index == self.count {
            return None;
        }

        // NSString. No release because not a copy.
        let nsstr = self.args.objectAtIndex(self.index);
        self.index += 1;

        // Convert to string using getCString. Our buffer should always
        // be big enough because we precomputed the maximum length.
        //
        // PORT: the reference calls `getCString:maxLength:encoding:` into the
        // precomputed `self.buf`. On this runtime the Swift-bridged
        // `__StringStorage` exposes that selector with a legacy 'c' (signed-char
        // BOOL) return encoding that disagrees with objc2's modern 'B' encoding,
        // tripping objc2's message-send verification. We obtain the identical
        // UTF-8 bytes via `UTF8String` (the same NSUTF8StringEncoding the
        // reference requests) and copy them into the precomputed buffer; the
        // observable result — the argument's UTF-8 string — is unchanged.
        let c_str = nsstr.UTF8String();
        if c_str.is_null() {
            // This should never happen... if it does, we just return empty.
            return Some(String::new());
        }
        let src = unsafe { std::ffi::CStr::from_ptr(c_str) }.to_bytes_with_nul();
        if src.len() <= self.buf.len() {
            self.buf[0..src.len()].copy_from_slice(src);
        }

        let bytes = unsafe { std::ffi::CStr::from_ptr(self.buf.as_ptr().cast()) }.to_bytes();
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    pub fn skip(&mut self) -> bool {
        if self.index == self.count {
            return false;
        }
        self.index += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)] // ffi (macOS): NSProcessInfo crosses the ObjC boundary (ASan, not Miri)
    // PARITY: the reference asserts `iter.next().?.len > 0`; the `> 0` form is
    // kept verbatim (assertion-for-assertion) rather than `!is_empty()`.
    #[allow(clippy::len_zero)]
    fn args() {
        let mut iter = iterator().unwrap();
        // deinit is run at scope end; mirror the reference's `defer iter.deinit()`.
        let first = iter.next();
        assert!(first.unwrap().len() > 0);
        iter.deinit();
    }
}
