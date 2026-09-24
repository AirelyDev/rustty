//! zlib-sys — FFI bindings to the vendored zlib 1.3.1 compression (`links = "z"`).
//!
//! PORT STATUS: vendor-proof slice
//!
//! R3: copies the SAME C library the reference uses (zlib 1.3.1) and builds it
//! standalone (`cc`; see `build.rs`). No pure-Rust substitution during parity
//! phases. The Phase-1 proof slice vendors zlib's `adler32` checksum unit under
//! `vendor/` and exposes the hand-written thin FFI below; the
//! `cargo test -p zlib-sys` known-answer test links and exercises the C symbol,
//! proving the vendor toolchain end-to-end.

#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int, c_uint, c_ulong, c_void};

extern "C" {
    /// Update a running Adler-32 checksum with `buf[0..len-1]` and return the
    /// updated checksum (upstream `adler32` in `vendor/adler32.c`). With a NULL
    /// `buf` it returns the initial seed value (`1`).
    ///
    /// # Safety
    /// `buf` must point to at least `len` readable bytes, or be NULL.
    pub fn adler32(adler: c_ulong, buf: *const u8, len: c_uint) -> c_ulong;
}

// ---------------------------------------------------------------------------
// Streaming inflate (DEFLATE decompression) FFI surface.
//
// This is the streaming inflate API (`inflateInit2_`/`inflate`/`inflateEnd`)
// plus the ABI-stable `z_stream` control block. It is the C-side primitive the
// reference's `std.compress.flate.Decompress` (zlib mode) covers: incremental
// decompression of a zlib-wrapped DEFLATE stream into a growable output buffer.
// ---------------------------------------------------------------------------

/// The zlib stream control block (`z_stream` / `z_stream_s`). The layout is the
/// long-stable public zlib ABI; we reproduce it exactly so the C inflate
/// routines read/write the correct fields. Fields we do not consume are still
/// declared so the struct size and offsets match the C definition.
#[repr(C)]
pub struct z_stream {
    /// next input byte
    pub next_in: *const u8,
    /// number of bytes available at next_in
    pub avail_in: c_uint,
    /// total number of input bytes read so far
    pub total_in: c_ulong,

    /// next output byte will go here
    pub next_out: *mut u8,
    /// remaining free space at next_out
    pub avail_out: c_uint,
    /// total number of bytes output so far
    pub total_out: c_ulong,

    /// last error message, NULL if no error
    pub msg: *const c_char,
    /// internal state, not visible by applications
    pub state: *mut c_void,

    /// used to allocate the internal state
    pub zalloc: *mut c_void,
    /// used to free the internal state
    pub zfree: *mut c_void,
    /// private data object passed to zalloc and zfree
    pub opaque: *mut c_void,

    /// best guess about the data type: binary or text for deflate, or the
    /// decoding state for inflate
    pub data_type: c_int,
    /// Adler-32 or CRC-32 value of the uncompressed data
    pub adler: c_ulong,
    /// reserved for future use
    pub reserved: c_ulong,
}

/// zlib version string the headers we vendored describe (`ZLIB_VERSION`).
/// `inflateInit2_` checks the first byte against the compiled library.
pub const ZLIB_VERSION: &[u8] = b"1.3.1\0";

/// `Z_OK` — operation completed without error.
pub const Z_OK: c_int = 0;
/// `Z_STREAM_END` — the end of the compressed stream was reached.
pub const Z_STREAM_END: c_int = 1;
/// `Z_NO_FLUSH` — the normal inflate flush mode.
pub const Z_NO_FLUSH: c_int = 0;
/// `Z_FINISH` — the deflate flush mode used for a single-shot compress: all
/// input is provided up front and all output is produced before returning
/// `Z_STREAM_END` (framegen's `deflate(&stream, Z_FINISH)`).
pub const Z_FINISH: c_int = 4;
/// `Z_DEFLATED` — the only supported compression method (`deflateInit2`'s
/// `method` arg).
pub const Z_DEFLATED: c_int = 8;
/// `Z_DEFAULT_COMPRESSION` — level `-1`, zlib's default (6). framegen passes
/// this as `deflateInit2`'s `level`.
pub const Z_DEFAULT_COMPRESSION: c_int = -1;
/// `Z_DEFAULT_STRATEGY` — the general-purpose deflate strategy (0). framegen
/// passes this as `deflateInit2`'s `strategy`.
pub const Z_DEFAULT_STRATEGY: c_int = 0;
/// Default window bits for a zlib-wrapped stream (`MAX_WBITS` = 15).
pub const MAX_WBITS: c_int = 15;
/// Raw DEFLATE window bits: a negative `windowBits` selects the raw stream
/// format (no zlib/gzip header or trailer). framegen uses `-MAX_WBITS`; the
/// reference `boo` decompresses with `.raw` — both are `-15`.
pub const RAW_WBITS: c_int = -MAX_WBITS;

extern "C" {
    /// `inflateInit2_`: initialize `strm` for decompression with `window_bits`
    /// (the public `inflateInit2` macro forwards version/size here).
    ///
    /// # Safety
    /// `strm` must point to a zeroed `z_stream`; `version`/`stream_size` must
    /// describe this crate's compiled zlib.
    pub fn inflateInit2_(
        strm: *mut z_stream,
        window_bits: c_int,
        version: *const c_char,
        stream_size: c_int,
    ) -> c_int;

    /// `inflate`: decompress as much as possible, updating the `next_*`/`avail_*`
    /// fields. Returns `Z_OK`, `Z_STREAM_END`, or a negative error code.
    ///
    /// # Safety
    /// `strm` must be a stream previously initialized by `inflateInit2_` with
    /// valid input/output buffers described by its fields.
    pub fn inflate(strm: *mut z_stream, flush: c_int) -> c_int;

    /// `inflateEnd`: release all dynamically allocated state for `strm`.
    ///
    /// # Safety
    /// `strm` must be a stream previously initialized by `inflateInit2_`.
    pub fn inflateEnd(strm: *mut z_stream) -> c_int;
}

// ---------------------------------------------------------------------------
// Streaming deflate (DEFLATE compression) FFI surface.
//
// The C-side primitive framegen's `main.c` uses to build the `framedata` asset:
// `deflateInit2_`/`deflate`/`deflateEnd` over the same ABI-stable `z_stream`,
// plus `compressBound` to size the output buffer. framegen selects RAW DEFLATE
// via `-MAX_WBITS`, level `Z_DEFAULT_COMPRESSION`, `memLevel = 8`, and
// `Z_DEFAULT_STRATEGY`.
// ---------------------------------------------------------------------------

extern "C" {
    /// `deflateInit2_`: initialize `strm` for compression (the public
    /// `deflateInit2` macro forwards version/size here).
    ///
    /// # Safety
    /// `strm` must point to a zeroed `z_stream`; `version`/`stream_size` must
    /// describe this crate's compiled zlib.
    pub fn deflateInit2_(
        strm: *mut z_stream,
        level: c_int,
        method: c_int,
        window_bits: c_int,
        mem_level: c_int,
        strategy: c_int,
        version: *const c_char,
        stream_size: c_int,
    ) -> c_int;

    /// `deflate`: compress as much as possible, updating the `next_*`/`avail_*`
    /// fields. With `Z_FINISH` and enough output space it returns
    /// `Z_STREAM_END`.
    ///
    /// # Safety
    /// `strm` must be a stream previously initialized by `deflateInit2_` with
    /// valid input/output buffers described by its fields.
    pub fn deflate(strm: *mut z_stream, flush: c_int) -> c_int;

    /// `deflateEnd`: release all dynamically allocated state for `strm`.
    ///
    /// # Safety
    /// `strm` must be a stream previously initialized by `deflateInit2_`.
    pub fn deflateEnd(strm: *mut z_stream) -> c_int;

    /// `compressBound`: an upper bound on the compressed size for `source_len`
    /// input bytes (used to size the single-shot output buffer).
    pub fn compressBound(source_len: c_ulong) -> c_ulong;
}

/// Error returned by [`deflate_raw`] when compression fails (a malformed init
/// or an unexpected non-`Z_STREAM_END` result — neither is reachable for the
/// framegen inputs, but we surface it rather than panic).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeflateError;

/// Compress `data` as a **raw** DEFLATE stream (no zlib/gzip wrapper), byte-for-byte
/// matching framegen's `main.c`:
///
/// ```text
/// deflateInit2(&stream, Z_DEFAULT_COMPRESSION, Z_DEFLATED, -MAX_WBITS, 8, Z_DEFAULT_STRATEGY);
/// deflate(&stream, Z_FINISH);   // one shot: all input, all output
/// ```
///
/// The output buffer is sized with [`compressBound`] up front so a single
/// `Z_FINISH` produces the whole stream. Round-trips with [`inflate_raw`].
pub fn deflate_raw(data: &[u8]) -> Result<Vec<u8>, DeflateError> {
    // SAFETY: a fully zeroed z_stream is the documented pre-init state.
    let mut strm: z_stream = unsafe { std::mem::zeroed() };

    // SAFETY: valid zeroed stream; params mirror framegen exactly (raw DEFLATE,
    // default level, method Z_DEFLATED, memLevel 8, default strategy).
    let rc = unsafe {
        deflateInit2_(
            &mut strm,
            Z_DEFAULT_COMPRESSION,
            Z_DEFLATED,
            RAW_WBITS,
            8,
            Z_DEFAULT_STRATEGY,
            ZLIB_VERSION.as_ptr() as *const c_char,
            std::mem::size_of::<z_stream>() as c_int,
        )
    };
    if rc != Z_OK {
        return Err(DeflateError);
    }

    // Guard so `deflateEnd` always runs, even on the error paths below.
    struct Guard(*mut z_stream);
    impl Drop for Guard {
        fn drop(&mut self) {
            // SAFETY: the stream was successfully initialized above.
            unsafe {
                deflateEnd(self.0);
            }
        }
    }
    let _guard = Guard(&mut strm);

    // SAFETY: compressBound is a pure size computation.
    let bound = unsafe { compressBound(data.len() as c_ulong) } as usize;
    let mut out: Vec<u8> = vec![0u8; bound];

    strm.next_in = data.as_ptr();
    strm.avail_in = data.len() as c_uint;
    strm.next_out = out.as_mut_ptr();
    strm.avail_out = out.len() as c_uint;

    // SAFETY: strm is initialized; in/out buffers are valid and bound-sized so a
    // single Z_FINISH suffices.
    let rc = unsafe { deflate(&mut strm, Z_FINISH) };
    if rc != Z_STREAM_END {
        return Err(DeflateError);
    }

    let produced = strm.total_out as usize;
    out.truncate(produced);
    Ok(out)
}

/// Error returned by [`inflate_zlib`] when the input is not a valid
/// zlib-wrapped DEFLATE stream or decompression otherwise fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InflateError;

/// Decompress a zlib-wrapped DEFLATE stream (`data`) into a freshly allocated
/// `Vec<u8>`, refusing to produce more than `max_size` output bytes.
///
/// This is the safe wrapper over the streaming inflate API. It grows the output
/// buffer incrementally (mirroring a streaming reader that appends remaining
/// bytes with a size limit) and returns [`InflateError`] on any malformed input
/// or if the decompressed size would exceed `max_size`.
pub fn inflate_zlib(data: &[u8], max_size: usize) -> Result<Vec<u8>, InflateError> {
    let mut out: Vec<u8> = Vec::new();
    inflate_impl(data, max_size, MAX_WBITS, &mut out)?;
    Ok(out)
}

/// The growable output an inflate appends to: a caller-owned byte list, so
/// the caller decides which allocator the decompressed bytes live in.
pub trait InflateOut {
    /// The current length.
    fn len(&self) -> usize;
    /// Whether the output is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Grow to `new_len` bytes (the new tail's contents are unspecified;
    /// inflate overwrites what it reports), or fail. A failure ends the
    /// inflate with [`InflateError`].
    fn try_grow(&mut self, new_len: usize) -> Result<(), InflateError>;
    /// Shrink to `new_len` bytes.
    fn truncate(&mut self, new_len: usize);
    /// The start of the buffer, valid for `len()` bytes.
    fn as_mut_ptr(&mut self) -> *mut u8;
}

impl InflateOut for Vec<u8> {
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn try_grow(&mut self, new_len: usize) -> Result<(), InflateError> {
        self.resize(new_len, 0);
        Ok(())
    }
    fn truncate(&mut self, new_len: usize) {
        Vec::truncate(self, new_len)
    }
    fn as_mut_ptr(&mut self) -> *mut u8 {
        Vec::as_mut_ptr(self)
    }
}

/// [`inflate_zlib`], appending the decompressed bytes to `out` (which must
/// be empty): the reference's `stream.reader.appendRemaining(alloc, &list,
/// .limited(max_size))` into a list the caller allocates. A failed growth of
/// `out` is an [`InflateError`], as a failed `appendRemaining` is.
pub fn inflate_zlib_into<O: InflateOut>(
    data: &[u8],
    max_size: usize,
    out: &mut O,
) -> Result<(), InflateError> {
    inflate_impl(data, max_size, MAX_WBITS, out)
}

/// Decompress a **raw** DEFLATE stream (no zlib/gzip wrapper) into a freshly
/// allocated `Vec<u8>`, refusing to produce more than `max_size` output bytes.
///
/// This is the exact counterpart of the reference `boo`'s
/// `std.compress.flate.Decompress.init(&src, .raw, &.{})`: raw DEFLATE selected
/// by the negative window bits [`RAW_WBITS`] (`-15`). Used to decompress the
/// `framedata` asset produced by the framegen raw-DEFLATE step.
pub fn inflate_raw(data: &[u8], max_size: usize) -> Result<Vec<u8>, InflateError> {
    let mut out: Vec<u8> = Vec::new();
    inflate_impl(data, max_size, RAW_WBITS, &mut out)?;
    Ok(out)
}

/// Shared streaming-inflate core, parameterized by `window_bits`
/// ([`MAX_WBITS`] = zlib-wrapped, [`RAW_WBITS`] = raw DEFLATE).
fn inflate_impl<O: InflateOut>(
    data: &[u8],
    max_size: usize,
    window_bits: c_int,
    out: &mut O,
) -> Result<(), InflateError> {
    // SAFETY: a fully zeroed z_stream is the documented pre-init state; all
    // pointer fields are NULL so zlib installs its own allocator.
    let mut strm: z_stream = unsafe { std::mem::zeroed() };

    // SAFETY: `strm` is a valid zeroed stream; ZLIB_VERSION/size describe our
    // compiled zlib. `window_bits` selects zlib-wrapped (+15) or raw (-15).
    let rc = unsafe {
        inflateInit2_(
            &mut strm,
            window_bits,
            ZLIB_VERSION.as_ptr() as *const c_char,
            std::mem::size_of::<z_stream>() as c_int,
        )
    };
    if rc != Z_OK {
        return Err(InflateError);
    }

    // Guard so `inflateEnd` always runs, even on the error paths below.
    struct Guard(*mut z_stream);
    impl Drop for Guard {
        fn drop(&mut self) {
            // SAFETY: the stream was successfully initialized above.
            unsafe {
                inflateEnd(self.0);
            }
        }
    }
    let _guard = Guard(&mut strm);

    strm.next_in = data.as_ptr();
    strm.avail_in = data.len() as c_uint;

    debug_assert_eq!(0, out.len());
    // Decompress in chunks, growing the output until the stream ends.
    let chunk: usize = 64 * 1024;
    loop {
        let prev_len = out.len();
        if prev_len >= max_size {
            // We have hit the cap without seeing stream end: too large.
            // Allow exactly max_size but anything beyond is a failure.
            // Probe whether more output remains by attempting one more byte.
        }
        let grow = chunk.min(max_size.saturating_sub(prev_len).max(1));
        out.try_grow(prev_len + grow)?;

        // SAFETY: out has `prev_len + grow` bytes; next_out points at the free
        // tail and avail_out is the free length.
        strm.next_out = unsafe { out.as_mut_ptr().add(prev_len) };
        strm.avail_out = grow as c_uint;

        // SAFETY: strm is initialized with valid in/out buffers.
        let rc = unsafe { inflate(&mut strm, Z_NO_FLUSH) };

        let produced = grow - strm.avail_out as usize;
        out.truncate(prev_len + produced);

        match rc {
            Z_STREAM_END => return Ok(()),
            Z_OK => {
                if strm.avail_out != 0 {
                    // inflate made no forward progress with output space left
                    // and no stream end: treat as malformed.
                    if produced == 0 && strm.avail_in == 0 {
                        return Err(InflateError);
                    }
                }
                if out.len() >= max_size {
                    // Output reached the cap but the stream did not end.
                    return Err(InflateError);
                }
                // Otherwise loop to provide more output space.
            }
            _ => return Err(InflateError),
        }
    }
}

/// Safe wrapper computing the zlib Adler-32 checksum of `data`.
///
/// Seeds with the zlib initial value (`1`) exactly as `adler32(0L, Z_NULL, 0)`
/// would return, then folds in the bytes. Result matches zlib bit-for-bit.
#[must_use]
pub fn adler32_checksum(data: &[u8]) -> u32 {
    // SAFETY: data.as_ptr() points to data.len() readable bytes; len fits c_uint
    // for any slice this crate's callers pass (proof slice uses tiny inputs).
    let sum = unsafe { adler32(1, data.as_ptr(), data.len() as c_uint) };
    sum as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known-answer test: zlib's documented Adler-32 of the ASCII string
    /// "Wikipedia" is 0x11E60398 (the canonical worked example). Linking and
    /// matching this proves the vendored C compiled, linked, and runs correctly
    /// (R3 vendor-toolchain proof).
    #[test]
    fn adler32_wikipedia_known_answer() {
        assert_eq!(adler32_checksum(b"Wikipedia"), 0x11E6_0398);
    }

    /// The empty input yields the seed value 1 (zlib invariant).
    #[test]
    fn adler32_empty_is_one() {
        assert_eq!(adler32_checksum(b""), 1);
    }

    /// A single byte: adler32("a") = 0x00620062 (sum1=0x62, sum2=0x62).
    #[test]
    fn adler32_single_byte() {
        assert_eq!(adler32_checksum(b"a"), 0x0062_0062);
    }

    /// The raw FFI seed behavior: adler32(0, NULL, 0) returns 1.
    #[test]
    fn adler32_ffi_null_seed() {
        // SAFETY: NULL buf with len 0 is the documented "return initial value".
        let seed = unsafe { adler32(0, std::ptr::null(), 0) };
        assert_eq!(seed as u32, 1);
    }

    /// Known-answer test for the inflate (decompression) surface: a zlib stream
    /// produced by `zlib.compress(b"hello, world! hello, world!", 9)` must
    /// inflate back to the original bytes. This proves the vendored inflate
    /// translation units link and run correctly end-to-end.
    #[test]
    fn inflate_zlib_known_answer() {
        let compressed: &[u8] = &[
            120, 218, 203, 72, 205, 201, 201, 215, 81, 40, 207, 47, 202, 73, 81, 84, 200, 64, 230,
            1, 0, 134, 251, 9, 115,
        ];
        let expected: &[u8] = b"hello, world! hello, world!";
        let out = inflate_zlib(compressed, 4 * 1024 * 1024).unwrap();
        assert_eq!(out.as_slice(), expected);
    }

    /// Malformed input is rejected.
    #[test]
    fn inflate_zlib_invalid_data() {
        let garbage: &[u8] = &[0xde, 0xad, 0xbe, 0xef, 0x00, 0x01, 0x02, 0x03];
        assert_eq!(inflate_zlib(garbage, 4 * 1024 * 1024), Err(InflateError));
    }

    /// Output exceeding the size cap is rejected.
    #[test]
    fn inflate_zlib_too_large() {
        let compressed: &[u8] = &[
            120, 218, 203, 72, 205, 201, 201, 215, 81, 40, 207, 47, 202, 73, 81, 84, 200, 64, 230,
            1, 0, 134, 251, 9, 115,
        ];
        // The decompressed output is 27 bytes; a cap of 10 must fail.
        assert_eq!(inflate_zlib(compressed, 10), Err(InflateError));
    }

    /// Round-trip known-answer test for the raw DEFLATE surface: `deflate_raw`
    /// (framegen's compressor) → `inflate_raw` (boo's decompressor) must recover
    /// the original bytes exactly, and the raw stream must NOT carry a zlib
    /// header (first byte is not `0x78`). Proves the vendored deflate/trees
    /// translation units link and run, and that framegen⇄boo agree on the raw
    /// format.
    #[test]
    fn deflate_raw_inflate_raw_round_trip() {
        // A payload with the boo separator + newlines, exercising real content.
        let original: &[u8] =
            b"frame one line1\nline2\x01frame two line1\nline2\x01frame three \xE2\x80\xA6 done";
        let compressed = deflate_raw(original).unwrap();
        // Raw DEFLATE has no zlib wrapper: a zlib stream would start 0x78.
        assert!(!compressed.is_empty());
        assert_ne!(compressed[0], 0x78);
        let out = inflate_raw(&compressed, 4 * 1024 * 1024).unwrap();
        assert_eq!(out.as_slice(), original);
    }

    /// The empty input round-trips through the raw surface (framegen never
    /// emits empty, but the primitive must be total).
    #[test]
    fn deflate_raw_empty_round_trip() {
        let compressed = deflate_raw(b"").unwrap();
        let out = inflate_raw(&compressed, 1024).unwrap();
        assert_eq!(out.as_slice(), b"");
    }

    // S39-F5: the reference's `libz` compiles all 15 zlib units
    // (`pkg/zlib/build.zig:61-77`). The two surfaces the port used to omit are
    // declared here, test-only: nothing in the tree calls them, so this crate
    // exposes no Rust API for them.
    extern "C" {
        fn gzopen(path: *const c_char, mode: *const c_char) -> *mut c_void;
        fn gzwrite(file: *mut c_void, buf: *const u8, len: c_uint) -> c_int;
        fn gzread(file: *mut c_void, buf: *mut u8, len: c_uint) -> c_int;
        fn gzclose(file: *mut c_void) -> c_int;
        fn inflateBackInit_(
            strm: *mut z_stream,
            window_bits: c_int,
            window: *mut u8,
            version: *const c_char,
            stream_size: c_int,
        ) -> c_int;
        fn inflateBack(
            strm: *mut z_stream,
            in_fn: unsafe extern "C" fn(*mut c_void, *mut *mut u8) -> c_uint,
            in_desc: *mut c_void,
            out_fn: unsafe extern "C" fn(*mut c_void, *mut u8, c_uint) -> c_int,
            out_desc: *mut c_void,
        ) -> c_int;
        fn inflateBackEnd(strm: *mut z_stream) -> c_int;
    }

    /// `gzwrite` a gzip file (`gzlib`/`gzwrite`/`gzclose`), then `gzread` it
    /// back (`gzread`): the `gz*` units are compiled into this `libz` and work.
    #[test]
    fn gz_file_api_round_trips() {
        let path = std::env::temp_dir().join(format!("rustty-s39-f5-{}.gz", std::process::id()));
        let cpath = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let payload: &[u8] = b"the gz file API is compiled into libz, as in the reference";

        // SAFETY: valid NUL-terminated path and mode; the handle is closed once.
        unsafe {
            let w = gzopen(cpath.as_ptr(), c"wb".as_ptr());
            assert!(!w.is_null(), "gzopen for writing");
            assert_eq!(
                gzwrite(w, payload.as_ptr(), payload.len() as c_uint),
                payload.len() as c_int
            );
            assert_eq!(gzclose(w), Z_OK);
        }
        // A gzip member on disk: the 1f 8b magic.
        let raw = std::fs::read(&path).unwrap();
        assert_eq!(&raw[..2], &[0x1f, 0x8b]);

        let mut back = vec![0u8; payload.len() + 16];
        // SAFETY: as above; `back` has room for `back.len()` bytes.
        let n = unsafe {
            let r = gzopen(cpath.as_ptr(), c"rb".as_ptr());
            assert!(!r.is_null(), "gzopen for reading");
            let n = gzread(r, back.as_mut_ptr(), back.len() as c_uint);
            assert_eq!(gzclose(r), Z_OK);
            n
        };
        let _ = std::fs::remove_file(&path);
        assert_eq!(n, payload.len() as c_int);
        assert_eq!(&back[..payload.len()], payload);
    }

    struct BackIn<'a> {
        data: &'a [u8],
        given: bool,
    }

    unsafe extern "C" fn back_in(desc: *mut c_void, buf: *mut *mut u8) -> c_uint {
        // SAFETY: `desc` is the `BackIn` passed to `inflateBack` below.
        let src = unsafe { &mut *(desc as *mut BackIn<'_>) };
        if src.given {
            return 0;
        }
        src.given = true;
        // SAFETY: zlib only reads through this pointer.
        unsafe { *buf = src.data.as_ptr() as *mut u8 };
        src.data.len() as c_uint
    }

    unsafe extern "C" fn back_out(desc: *mut c_void, buf: *mut u8, len: c_uint) -> c_int {
        // SAFETY: `desc` is the output `Vec` passed to `inflateBack` below, and
        // zlib hands `len` readable bytes at `buf`.
        let out = unsafe { &mut *(desc as *mut Vec<u8>) };
        out.extend_from_slice(unsafe { core::slice::from_raw_parts(buf, len as usize) });
        0
    }

    /// `inflateBack` (`infback.c`) decodes a raw DEFLATE stream.
    #[test]
    fn infback_decodes_a_raw_deflate_stream() {
        let original: &[u8] = b"inflateBack is compiled into libz, as in the reference";
        let compressed = deflate_raw(original).unwrap();

        let mut window = vec![0u8; 1 << MAX_WBITS];
        // SAFETY: a zeroed z_stream is the documented pre-init state.
        let mut strm: z_stream = unsafe { core::mem::zeroed() };
        let mut input = BackIn {
            data: &compressed,
            given: false,
        };
        let mut out: Vec<u8> = Vec::new();
        // SAFETY: `window` is 2^15 bytes as windowBits 15 requires; the
        // descriptors outlive the call; the stream is ended once.
        unsafe {
            assert_eq!(
                inflateBackInit_(
                    &mut strm,
                    MAX_WBITS,
                    window.as_mut_ptr(),
                    ZLIB_VERSION.as_ptr() as *const c_char,
                    core::mem::size_of::<z_stream>() as c_int,
                ),
                Z_OK
            );
            let rc = inflateBack(
                &mut strm,
                back_in,
                (&mut input as *mut BackIn<'_>).cast(),
                back_out,
                (&mut out as *mut Vec<u8>).cast(),
            );
            assert_eq!(inflateBackEnd(&mut strm), Z_OK);
            assert_eq!(rc, Z_STREAM_END);
        }
        assert_eq!(out.as_slice(), original);
    }
}
