//! Kitty clipboard protocol (OSC 5522) write transactions: the
//! stateful accumulation of wdata chunks and walias aliases until the
//! commit packet arrives. See clipboard.zig for the protocol overview.
//!
//! PORTS: ghostty src/terminal/kitty/clipboard_write.zig (B5 end state:
//! `7a940ec02` write transactions, `5350d4a5f`/`70f006575` configurable
//! limit with REJECT semantics — the in-range truncation engine and the
//! drop-invalid-chunk behavior were both reverted and are NOT ported —
//! `e8d8945b5` text-input validation, `bc2f7d7d2` streaming strict base64)

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const assert = @import("../../quirks.zig").inlineAssert;
//   const Allocator = std.mem.Allocator;
//   const simd = @import("../../simd/main.zig");
//   const clipboard = @import("../clipboard.zig");
//   const clipboard_command = @import("clipboard_command.zig");
// The transaction arena (`arena: std.heap.ArenaAllocator`) is the crate-wide
// global-allocator treatment: every arena-duped slice is an owned `Vec<u8>`
// and `deinit` maps to drop.
#![allow(non_upper_case_globals)]

use crate::clipboard;
use crate::kitty::clipboard_command;
use rustty_simd as simd;
use rustty_util::quirks::inlineAssert as assert;

// const Metadata = clipboard_command.Metadata;
// const Payload = clipboard_command.Payload;
// const max_mime_len = clipboard_command.max_mime_len;
use clipboard_command::{max_mime_len, Metadata, Payload};
use clipboard_command::{Operation, PayloadError};

// const log = std.log.scoped(.kitty_clipboard);

/// Default maximum total decoded bytes accumulated by one write
/// transaction. Embedders can override this per-transaction via
/// WriteState.Options (Ghostty exposes it as the
/// `clipboard-write-limit-bytes` configuration).
pub const max_write_size: usize = 64 * 1024 * 1024;

/// Maximum MIME types and aliases per write transaction.
pub const max_write_mimes: usize = 64;
pub const max_write_aliases: usize = 64;

/// One MIME representation of committed clipboard data. This is the
/// same type the clipboard write effect consumes so committed contents
/// can be passed through directly.
pub use clipboard::Content;

/// `WriteState.data`'s error set: `error{ OutOfMemory, TooLarge, Invalid }`.
// PORT NOTE: `OutOfMemory` carried for parity; only `TooLarge`/`Invalid` are
// produced by the global-allocator port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataError {
    OutOfMemory,
    TooLarge,
    Invalid,
}

/// `WriteState.alias`'s error set: `error{ OutOfMemory, Invalid }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AliasError {
    OutOfMemory,
    Invalid,
}

/// `WriteState.commit`'s error set: `error{ OutOfMemory, Invalid }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitError {
    OutOfMemory,
    Invalid,
}

/// The state of one in-flight write transaction: a single `type=write`
/// plus all the `wdata` chunks and `walias` aliases until completion
/// or error.
pub struct WriteState {
    // arena: std.heap.ArenaAllocator,
    // (owned Vecs; no arena field — see the module PORT NOTE)
    pub loc: clipboard::Location,
    pub id: Vec<u8>,
    pub pw: Vec<u8>,
    pub name: Vec<u8>,
    // spool: std.ArrayListUnmanaged(u8) = .empty,
    spool: Vec<u8>,
    // entries: std.ArrayListUnmanaged(Entry) = .empty,
    entries: Vec<Entry>,
    // aliases: std.ArrayListUnmanaged(Alias) = .empty,
    aliases: Vec<Alias>,

    /// Maximum total decoded bytes this transaction will accumulate.
    /// Captured at init so a change to the configured limit doesn't
    /// apply to a transaction already in flight.
    pub max_size: usize,

    /// Index into entries currently receiving data.
    // current: ?usize = null,
    current: Option<usize>,

    /// Decodes the concatenated payload stream of the entry currently
    /// receiving data. Per the spec's "Encoding of payloads" section,
    /// individual wdata packet payloads split one base64 stream at
    /// arbitrary boundaries; only the concatenation per MIME type must
    /// be valid, correctly padded base64.
    // decoder: simd.base64.Streaming = .{},
    // PORT NOTE (assessment A hazard #5): the WriteState OWNS its streaming
    // decoder state and every `feed` receives a fresh `&mut` output slice —
    // the decoder never holds a borrow of the spool between packets.
    decoder: simd::base64::Streaming,
}

/// PORT NOTE: the reference nests this as `WriteState.Options`; flattened to
/// module scope (Rust structs cannot nest type declarations).
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Maximum total decoded bytes accumulated by the transaction.
    // max_size: usize = max_write_size,
    pub max_size: usize,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            max_size: max_write_size,
        }
    }
}

// const Entry = struct { ... };
struct Entry {
    /// Owned by the transaction arena.
    mime: Vec<u8>,
    // start: usize = 0,
    start: usize,
    // len: usize = 0,
    len: usize,
}

// const Alias = struct { ... };
struct Alias {
    /// Owned by the transaction arena.
    alias: Vec<u8>,
    // PORT NOTE: the reference's `target` is one arena-duped slice SHARED by
    // every alias registered in the same call (pointer assignment on
    // overwrite); the port clones the bytes per alias — same observable
    // mapping. PERF(port): one small allocation per alias vs one per call.
    target: Vec<u8>,
}

/// The result of a committed transaction. All slices borrow the
/// WriteState's memory and are valid until it is deinited.
// PORT NOTE: the reference nests this as `WriteState.Committed`; flattened.
// The borrow is expressed in the lifetime: a `Committed` keeps the
// `WriteState` mutably borrowed until dropped.
pub struct Committed<'a> {
    pub loc: clipboard::Location,
    pub id: &'a [u8],
    pub pw: &'a [u8],
    pub name: &'a [u8],
    pub contents: Vec<Content<'a>>,
}

// pub fn deinit(self: *const Committed, alloc: Allocator) void {
//     alloc.free(self.contents);
// }
// PORT NOTE: `contents` frees on drop; no deinit.

impl WriteState {
    /// Begin a transaction from a type=write packet.
    // pub fn init(alloc: Allocator, meta: *const Metadata, opts: Options)
    //     Allocator.Error!WriteState {
    pub fn init(meta: &Metadata, opts: Options) -> WriteState {
        assert(meta.op == Operation::Write);
        // var arena: std.heap.ArenaAllocator = .init(alloc);
        // const id = try arena.allocator().dupe(u8, meta.id); (etc.)
        WriteState {
            loc: meta.loc,
            id: meta.id.clone(),
            pw: meta.pw.clone(),
            name: meta.name.clone(),
            spool: Vec::new(),
            entries: Vec::new(),
            aliases: Vec::new(),
            max_size: opts.max_size,
            current: None,
            decoder: simd::base64::Streaming::default(),
        }
    }

    // pub fn deinit(self: *WriteState, alloc: Allocator) void { ... }
    // PORT NOTE: all owned state frees on drop; no deinit.

    /// Accumulate one wdata chunk carrying data for meta.mime (which
    /// must be non-empty; an empty mime is a commit, not data).
    ///
    /// Returns error.TooLarge when the transaction exceeds max_size
    /// and error.Invalid when the payload stream is not valid base64.
    /// The caller must fail the whole transaction with EFBIG or EINVAL
    /// respectively and abort it, as required by the protocol.
    // pub fn data(self: *WriteState, alloc: Allocator, meta: *const Metadata,
    //     payload: []const u8) error{ OutOfMemory, TooLarge, Invalid }!void {
    pub fn data(&mut self, meta: &Metadata, payload: &[u8]) -> Result<(), DataError> {
        assert(meta.op == Operation::Wdata);
        assert(!meta.mime.is_empty());
        // Switch the receiving entry if this chunk is for a different
        // MIME type than the last one.
        // entry: { ... }
        'entry: {
            if let Some(idx) = self.current {
                if self.entries[idx].mime == meta.mime {
                    break 'entry;
                }

                // Finalize the previous region. Its concatenated
                // stream must have ended on a complete base64 group,
                // otherwise the data was not correctly padded.
                self.finish_current().map_err(|_| DataError::Invalid)?;
                let spool_len = self.spool.len();
                let entry = &mut self.entries[idx];
                entry.len = spool_len - entry.start;
            }

            // Re-using an earlier MIME type starts a fresh region,
            // overwriting the previous mapping.
            let spool_len = self.spool.len();
            let mut reused = false;
            for (idx, entry) in self.entries.iter_mut().enumerate() {
                if entry.mime == meta.mime {
                    entry.start = spool_len;
                    entry.len = 0;
                    self.current = Some(idx);
                    reused = true;
                    break;
                }
            }
            if reused {
                break 'entry;
            }

            if self.entries.len() >= max_write_mimes {
                // log.warn("clipboard write has too many MIME types, ignoring mime={s}",
                //     .{meta.mime});
                self.current = None;
                return Ok(());
            }

            self.entries.push(Entry {
                mime: meta.mime.clone(),
                start: self.spool.len(),
                len: 0,
            });
            self.current = Some(self.entries.len() - 1);
        }

        // The payloads for one MIME region concatenate into a single
        // strict base64 stream, decoded directly into the spool's
        // unused capacity. Invalid data aborts the transaction; per
        // the spec it must not be silently discarded "since that
        // turns corrupted data into apparently valid data".
        // try self.spool.ensureUnusedCapacity(alloc, self.decoder.maxLen(payload));
        // const decoded = self.decoder.feed(payload, self.spool.unusedCapacitySlice())
        //     catch return error.Invalid;
        //
        // PORT NOTE: the reference decodes into the spool's UNINITIALIZED
        // unused capacity and bumps `items.len` afterwards; the safe port
        // zero-extends the Vec first (PERF(port): one memset of the reserved
        // region per chunk), feeds into the initialized tail, then trims —
        // the decoder gets a per-call `&mut [u8]`, never a held borrow
        // (hazard #5).
        let start = self.spool.len();
        let reserve = self.decoder.max_len(payload);
        self.spool.resize(start + reserve, 0);
        let decoded_len = {
            let spool_tail = &mut self.spool[start..];
            match self.decoder.feed(payload, spool_tail) {
                Ok(decoded) => decoded.len(),
                Err(_) => {
                    self.spool.truncate(start);
                    return Err(DataError::Invalid);
                }
            }
        };

        // The limit covers all decoded data in the transaction. Going
        // over it aborts the entire write; partial clipboard contents
        // must never reach the embedder.
        // const remaining = self.max_size -| self.spool.items.len;
        let remaining = self.max_size.saturating_sub(start);
        if decoded_len > remaining {
            self.spool.truncate(start);
            return Err(DataError::TooLarge);
        }
        // self.spool.items.len += decoded.len;
        self.spool.truncate(start + decoded_len);
        Ok(())
    }

    /// Finish the decode stream of the entry currently receiving
    /// data. Returns error.Invalid when the stream ends mid-group,
    /// i.e. the concatenated payload was not correctly padded.
    // fn finishCurrent(self: *WriteState) error{Invalid}!void {
    fn finish_current(&mut self) -> Result<(), simd::base64::Base64Invalid> {
        self.decoder.finish()
    }

    /// Register aliases from a walias packet: meta.mime is the target
    /// (the type that carries data) and the payload is a base64-encoded,
    /// whitespace-separated list of aliases. Returns error.Invalid for
    /// an undecodable payload, which aborts the transaction with EINVAL.
    // pub fn alias(self: *WriteState, alloc: Allocator, meta: *const Metadata,
    //     payload: []const u8) error{ OutOfMemory, Invalid }!void {
    pub fn alias(&mut self, meta: &Metadata, payload: &[u8]) -> Result<(), AliasError> {
        assert(meta.op == Operation::Walias);
        assert(!meta.mime.is_empty());
        // const decoded = try Payload.init(alloc, payload);
        // defer decoded.deinit(alloc);
        let decoded = Payload::init(payload).map_err(|err| match err {
            PayloadError::OutOfMemory => AliasError::OutOfMemory,
            PayloadError::Invalid => AliasError::Invalid,
        })?;
        if !decoded.is_valid_utf8() {
            return Err(AliasError::Invalid);
        }
        let mut it = decoded.mime_iterator();

        // Copy the target only if at least one valid alias exists.
        // const target: []const u8 = target: { ... };
        let target: Vec<u8> = 'target: {
            while let Some(name) = it.next() {
                if name.len() > max_mime_len {
                    continue;
                }
                break 'target meta.mime.clone();
            }

            // If we didn't find a target then ignore it.
            return Ok(());
        };

        // Rewind so the alias that satisfied the check above is
        // associated too.
        it.reset();

        // Associate the aliases
        while let Some(name) = it.next() {
            if name.len() > max_mime_len {
                continue;
            }

            // A repeated alias overwrites its previous target.
            // for (self.aliases.items) |*a| { ... } else { ... }
            let mut matched = false;
            for a in self.aliases.iter_mut() {
                if a.alias == name {
                    a.target = target.clone();
                    matched = true;
                    break;
                }
            }
            if !matched {
                if self.aliases.len() >= max_write_aliases {
                    // log.warn("clipboard write has too many aliases, ignoring", .{});
                    return Ok(());
                }

                self.aliases.push(Alias {
                    alias: name.to_vec(),
                    target: target.clone(),
                });
            }
        }
        Ok(())
    }

    /// Commit the transaction (a wdata packet without a MIME type).
    /// The caller must use the result, call Committed.deinit, and then
    /// deinit this state. Returns error.Invalid when the last region's
    /// concatenated payload was not correctly padded; the caller must
    /// fail the transaction with EINVAL and abort it.
    // pub fn commit(self: *WriteState, alloc: Allocator)
    //     error{ OutOfMemory, Invalid }!Committed {
    pub fn commit(&mut self) -> Result<Committed<'_>, CommitError> {
        // Finalize the region receiving data.
        if let Some(idx) = self.current {
            self.finish_current().map_err(|_| CommitError::Invalid)?;
            let spool_len = self.spool.len();
            let entry = &mut self.entries[idx];
            entry.len = spool_len - entry.start;
            self.current = None;
        }

        // Resolve the final MIME map: entries in arrival order, then
        // aliases applied sequentially against the evolving map so
        // chained aliases work like kitty's dict iteration. An alias
        // whose target has no mapping is dropped; an alias colliding
        // with an existing name overwrites it.
        let mut contents: Vec<Content<'_>> =
            Vec::with_capacity(self.entries.len() + self.aliases.len());

        for entry in self.entries.iter() {
            contents.push(Content {
                mime: entry.mime.as_slice(),
                data: &self.spool[entry.start..][0..entry.len],
            });
        }

        for a in self.aliases.iter() {
            // const target: Content = target: { ... continue; };
            let target: Content<'_> = match contents
                .iter()
                .find(|c| c.mime == a.target.as_slice())
                .copied()
            {
                Some(c) => c,
                None => continue,
            };

            // for (contents.items) |*c| { ... } else { appendAssumeCapacity }
            let mut matched = false;
            for c in contents.iter_mut() {
                if c.mime == a.alias.as_slice() {
                    c.data = target.data;
                    matched = true;
                    break;
                }
            }
            if !matched {
                contents.push(Content {
                    mime: a.alias.as_slice(),
                    data: target.data,
                });
            }
        }

        Ok(Committed {
            loc: self.loc,
            id: &self.id,
            pw: &self.pw,
            name: &self.name,
            contents,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // (Zig `.{ .op = .wdata, .mime = "text/plain" }` shorthand.)
    fn wdata(mime: &[u8]) -> Metadata {
        Metadata {
            mime: mime.to_vec(),
            ..Metadata::new(Operation::Wdata)
        }
    }

    // test "write: basic transaction"
    #[test]
    fn write_basic_transaction() {
        // const begin_meta: Metadata = .{ .op = .write, .id = "42" };
        let begin_meta = Metadata {
            id: b"42".to_vec(),
            ..Metadata::new(Operation::Write)
        };
        let mut state = WriteState::init(&begin_meta, Options::default());

        state
            .data(&wdata(b"text/plain"), b"R2hvc3R0eQ==") // "Ghostty"
            .unwrap();

        let committed = state.commit().unwrap();
        assert_eq!(b"42".as_slice(), committed.id);
        assert!(committed.loc == crate::clipboard::Location::Standard);
        assert_eq!(1usize, committed.contents.len());
        assert_eq!(b"text/plain".as_slice(), committed.contents[0].mime);
        assert_eq!(b"Ghostty".as_slice(), committed.contents[0].data);
    }

    // test "write: chunked data accumulates"
    #[test]
    fn write_chunked_data_accumulates() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options::default());

        state.data(&wdata(b"text/plain"), b"SGVsbG8=").unwrap(); // "Hello"
        state.data(&wdata(b"text/plain"), b"V29ybGQ=").unwrap(); // "World"

        let committed = state.commit().unwrap();
        assert_eq!(b"HelloWorld".as_slice(), committed.contents[0].data);
    }

    // test "write: multiple mimes in order"
    #[test]
    fn write_multiple_mimes_in_order() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options::default());

        state.data(&wdata(b"text/plain"), b"YQ==").unwrap(); // "a"
        state.data(&wdata(b"text/html"), b"Yg==").unwrap(); // "b"

        let committed = state.commit().unwrap();
        assert_eq!(2usize, committed.contents.len());
        assert_eq!(b"text/plain".as_slice(), committed.contents[0].mime);
        assert_eq!(b"a".as_slice(), committed.contents[0].data);
        assert_eq!(b"text/html".as_slice(), committed.contents[1].mime);
        assert_eq!(b"b".as_slice(), committed.contents[1].data);
    }

    // test "write: reused mime overwrites"
    #[test]
    fn write_reused_mime_overwrites() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options::default());

        state.data(&wdata(b"text/plain"), b"YQ==").unwrap(); // "a"
        state.data(&wdata(b"text/html"), b"Yg==").unwrap(); // "b"
        state.data(&wdata(b"text/plain"), b"Yw==").unwrap(); // "c"

        let committed = state.commit().unwrap();
        assert_eq!(2usize, committed.contents.len());
        // Position preserved, data replaced.
        assert_eq!(b"text/plain".as_slice(), committed.contents[0].mime);
        assert_eq!(b"c".as_slice(), committed.contents[0].data);
    }

    // test "write: invalid base64 chunk aborts the transaction"
    #[test]
    fn write_invalid_base64_chunk_aborts_the_transaction() {
        // Invalid characters anywhere in the stream are error.Invalid,
        // which the handlers turn into an EINVAL abort. These mirror
        // kitty's own tests for the spec change.
        let invalid_payloads: [&[u8]; 3] =
            [b"!!!", b"SGVs!!!bG8=", b"\nZGF0YSB3aXRoIGEgbmV3bGluZQ=="];
        for payload in invalid_payloads {
            let begin_meta = Metadata::new(Operation::Write);
            let mut state = WriteState::init(&begin_meta, Options::default());
            assert_eq!(
                Err(DataError::Invalid),
                state.data(&wdata(b"text/plain"), payload),
            );
        }

        // Also after an earlier valid chunk for the same MIME type.
        {
            let begin_meta = Metadata::new(Operation::Write);
            let mut state = WriteState::init(&begin_meta, Options::default());
            state.data(&wdata(b"text/plain"), b"Z29vZA==").unwrap(); // "good"
            assert_eq!(
                Err(DataError::Invalid),
                state.data(&wdata(b"text/plain"), b"SGVs!!!bG8="),
            );
        }
    }

    // test "write: chunks split one base64 stream at arbitrary boundaries"
    #[test]
    fn write_chunks_split_one_base64_stream_at_arbitrary_boundaries() {
        // The concatenation of all payloads per MIME type is the base64
        // stream; individual packets need not be a multiple of four bytes.
        let encoded = b"c29tZSBkYXRh"; // "some data"
        let splits: [&[usize]; 2] = [&[3, 7], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]];
        for split in splits {
            let begin_meta = Metadata::new(Operation::Write);
            let mut state = WriteState::init(&begin_meta, Options::default());

            let mut prev: usize = 0;
            for &end in split {
                state
                    .data(&wdata(b"text/plain"), &encoded[prev..end])
                    .unwrap();
                prev = end;
            }
            state.data(&wdata(b"text/plain"), &encoded[prev..]).unwrap();

            let committed = state.commit().unwrap();
            assert_eq!(b"some data".as_slice(), committed.contents[0].data);
        }
    }

    // test "write: incorrectly padded stream aborts at commit"
    #[test]
    fn write_incorrectly_padded_stream_aborts_at_commit() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options::default());

        // "Hello" without its final padding byte: every chunk decodes,
        // but the stream ends mid-group so the commit reports it.
        state.data(&wdata(b"text/plain"), b"SGVsbG8").unwrap();
        assert!(matches!(state.commit(), Err(CommitError::Invalid)));
    }

    // test "write: incorrectly padded stream aborts at MIME switch"
    #[test]
    fn write_incorrectly_padded_stream_aborts_at_mime_switch() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options::default());

        state.data(&wdata(b"text/plain"), b"SGVsbG8").unwrap();
        assert_eq!(
            Err(DataError::Invalid),
            state.data(&wdata(b"text/html"), b"PGI+aGk8L2I+"),
        );
    }

    // test "write: independently padded chunks accumulate"
    #[test]
    fn write_independently_padded_chunks_accumulate() {
        // A packet payload ending exactly at terminal padding resets the
        // stream, so clients that base64-encode every chunk independently
        // keep working (matching kitty, which resets its streaming
        // decoder on EOF). Padding followed by more data within a single
        // packet stays invalid.
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options::default());

        state.data(&wdata(b"text/plain"), b"Z29vZA==").unwrap(); // "good"
        state.data(&wdata(b"text/plain"), b"bW9yZQ==").unwrap(); // "more"

        let committed = state.commit().unwrap();
        assert_eq!(b"goodmore".as_slice(), committed.contents[0].data);
    }

    // test "write: data after padding within one chunk aborts"
    #[test]
    fn write_data_after_padding_within_one_chunk_aborts() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options::default());

        assert_eq!(
            Err(DataError::Invalid),
            state.data(&wdata(b"text/plain"), b"Z29vZA==bW9yZQ=="),
        );
    }

    // test "write: aliases resolve at commit"
    #[test]
    fn write_aliases_resolve_at_commit() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options::default());

        state.data(&wdata(b"text/plain"), b"R2hvc3R0eQ==").unwrap(); // "Ghostty"

        // Alias "TEXT UTF8_STRING" -> text/plain.
        let alias_meta = Metadata {
            mime: b"text/plain".to_vec(),
            ..Metadata::new(Operation::Walias)
        };
        state
            .alias(&alias_meta, b"VEVYVCBVVEY4X1NUUklORw==")
            .unwrap();

        let committed = state.commit().unwrap();
        assert_eq!(3usize, committed.contents.len());
        assert_eq!(b"TEXT".as_slice(), committed.contents[1].mime);
        assert_eq!(b"Ghostty".as_slice(), committed.contents[1].data);
        assert_eq!(b"UTF8_STRING".as_slice(), committed.contents[2].mime);
        assert_eq!(b"Ghostty".as_slice(), committed.contents[2].data);
    }

    // test "write: alias payload must be valid utf8"
    #[test]
    fn write_alias_payload_must_be_valid_utf8() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options::default());

        let alias_meta = Metadata {
            mime: b"text/plain".to_vec(),
            ..Metadata::new(Operation::Walias)
        };
        // Valid base64 encoding of a single 0xff byte.
        assert_eq!(Err(AliasError::Invalid), state.alias(&alias_meta, b"/w=="));
    }

    // test "write: default limit when unset"
    #[test]
    fn write_default_limit_when_unset() {
        let begin_meta = Metadata::new(Operation::Write);
        let state = WriteState::init(&begin_meta, Options::default());
        assert_eq!(max_write_size, state.max_size);
    }

    // test "write: text over limit rejects transaction"
    #[test]
    fn write_text_over_limit_rejects_transaction() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options { max_size: 8 });

        assert_eq!(
            Err(DataError::TooLarge),
            state.data(&wdata(b"text/plain"), b"SGVsbG9Xb3JsZA=="), // "HelloWorld"
        );
    }

    // test "write: text crossing limit across chunks rejects transaction"
    #[test]
    fn write_text_crossing_limit_across_chunks_rejects_transaction() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options { max_size: 8 });

        state.data(&wdata(b"text/plain"), b"SGVsbG8=").unwrap(); // "Hello"
        assert_eq!(
            Err(DataError::TooLarge),
            state.data(&wdata(b"text/plain"), b"V29ybGQ="), // "World"
        );
    }

    // test "write: data exactly at limit is accepted"
    #[test]
    fn write_data_exactly_at_limit_is_accepted() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options { max_size: 5 });

        state.data(&wdata(b"text/plain"), b"SGVsbG8=").unwrap(); // "Hello"

        let committed = state.commit().unwrap();
        assert_eq!(b"Hello".as_slice(), committed.contents[0].data);
    }

    // test "write: non-text under limit is unaffected"
    #[test]
    fn write_non_text_under_limit_is_unaffected() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options { max_size: 8 });

        state.data(&wdata(b"image/png"), b"iVBORw==").unwrap(); // "\x89PNG"

        let committed = state.commit().unwrap();
        assert_eq!(b"\x89PNG".as_slice(), committed.contents[0].data);
    }

    // test "write: non-text over limit rejects transaction"
    #[test]
    fn write_non_text_over_limit_rejects_transaction() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options { max_size: 8 });

        assert_eq!(
            Err(DataError::TooLarge),
            state.data(&wdata(b"image/png"), b"SGVsbG9Xb3JsZA=="), // "HelloWorld"
        );
    }

    // test "write: total data across MIME types is limited"
    #[test]
    fn write_total_data_across_mime_types_is_limited() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options { max_size: 8 });

        state.data(&wdata(b"text/plain"), b"SGVsbG8=").unwrap(); // "Hello"

        assert_eq!(
            Err(DataError::TooLarge),
            state.data(&wdata(b"image/png"), b"V29ybGQ="), // "World"
        );
    }

    // test "write: alias without data target is dropped"
    #[test]
    fn write_alias_without_data_target_is_dropped() {
        let begin_meta = Metadata::new(Operation::Write);
        let mut state = WriteState::init(&begin_meta, Options::default());

        let alias_meta = Metadata {
            mime: b"text/plain".to_vec(),
            ..Metadata::new(Operation::Walias)
        };
        state.alias(&alias_meta, b"VEVYVA==").unwrap(); // "TEXT"

        let committed = state.commit().unwrap();
        assert_eq!(0usize, committed.contents.len());
    }
}
