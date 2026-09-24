//! rustty-crash::sentry_envelope
//!
//! PORTS: ghostty src/crash/sentry_envelope.zig — the Sentry Envelope
//! reader/serializer (8 reference tests, assertion-for-assertion).
//!
//! PORT STATUS: done
#![allow(non_snake_case)]

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
//
// PORT NOTE: `std.json` maps to `crate::json`, a port of the `std.json.Value`
// parse and minified stringify this file uses (`std.json.ObjectMap` is a
// `StringArrayHashMap(Value)`, insertion-order preserving, as `IndexMap` is).
// CORRECTION (LEDGER S79-F3, S79-F4): this used to say "`std.json` maps to
// `serde_json` with the `preserve_order` feature". serde_json differs from
// `std.json` on duplicate keys, out-of-`i64` integers, overflowing floats,
// float printing and nesting depth; see `crate::json`'s header.
// CORRECTION (UD182): the nesting-depth difference is not "`std.json` has no
// limit". `std.json` parses any depth but its `Stringify` panics past its
// documented 256 levels; the port parses any depth and `serialize` returns an
// `InvalidInput` error past 256 (reference defect, R13). The
// `std.Io.Reader` surface (`streamDelimiterLimit`/`discardDelimiterInclusive`/
// `streamExact`/`takeByte`) is reproduced over `std::io::BufRead` by the
// private helpers at the bottom of this file, with the reference's exact
// error-variant semantics.

use std::io::BufRead;

// const log = std.log.scoped(.sentry_envelope);

/// PORT NOTE: `std.json.ObjectMap` — the decoded JSON object map.
pub type ObjectMap = crate::json::ObjectMap;

/// PORT NOTE: the Zig error set of the envelope operations (the inferred `!`
/// unions), reified. The named variants preserve the reference error tags;
/// `Io`/`Json` carry the reader/writer (`error.ReadFailed`/`error.WriteFailed`)
/// and `std.json` parse errors.
#[derive(Debug)]
pub enum Error {
    EnvelopeMalformedHeaders,
    EnvelopeItemMalformedHeaders,
    EnvelopeItemTypeMissing,
    EnvelopeItemLengthMalformed,
    EnvelopeItemPayloadTooShort,
    EnvelopeItemPayloadNoNewline,
    /// PORT NOTE: `error.StreamTooLong` propagated by `parseHeader` when the
    /// envelope header line exceeds its limit.
    StreamTooLong,
    Io(std::io::Error),
    /// A `std.json` parse error (`error.SyntaxError`,
    /// `error.UnexpectedEndOfInput`, `error.DuplicateField`).
    Json(crate::json::ParseError),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<crate::json::ParseError> for Error {
    fn from(err: crate::json::ParseError) -> Error {
        Error::Json(err)
    }
}

impl From<StreamErr> for Error {
    fn from(err: StreamErr) -> Error {
        match err {
            StreamErr::StreamTooLong => Error::StreamTooLong,
            StreamErr::ReadFailed(e) => Error::Io(e),
            // Callers handle EndOfStream explicitly (the reference `switch`es
            // on it before the `else => return err` arm); mapped for the
            // propagating arms' completeness.
            StreamErr::EndOfStream => Error::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "EndOfStream",
            )),
        }
    }
}

/// The Sentry Envelope format: https://develop.sentry.dev/sdk/envelopes/
///
/// The envelope is our primary crash report format since use the Sentry
/// client. It is designed and created by Sentry but is an open format
/// in that it is publicly documented and can be used by any system. This
/// lets us utilize the Sentry client for crash capture but also gives us
/// the opportunity to migrate to another system if we need to, and doesn't
/// force any user or developer to use Sentry the SaaS if they don't want
/// to.
///
/// This struct implements reading the envelope format (writing is not needed
/// currently but can be added later). It is incomplete; I only implemented
/// what I needed at the time.
pub struct Envelope {
    // The arena that the envelope is allocated in. All items are welcome
    // to use this allocator for their data, which is freed on deinit.
    //
    // PORT NOTE: `arena: std.heap.ArenaAllocator` — in Rust the headers/items own
    // their data directly; `deinit` is Drop.
    //
    /// The headers of the envelope decoded into a json ObjectMap.
    pub headers: ObjectMap,

    /// The items in the envelope in the order they're encoded.
    pub items: Vec<Item>,
}

impl Envelope {
    /// Parse an envelope from a reader.
    ///
    /// The full envelope must fit in memory for this to succeed. This
    /// will always copy the data from the reader into memory, even if the
    /// reader is already in-memory (i.e. a FixedBufferStream). This
    /// simplifies memory lifetimes at the expense of a copy, but envelope
    /// parsing in our use case is not a hot path.
    // pub fn parse(
    //     alloc_gpa: Allocator,
    //     reader: *std.Io.Reader,
    // ) !Envelope {
    pub fn parse<R: BufRead>(reader: &mut R) -> Result<Envelope, Error> {
        // We use an arena allocator to read from reader. We pair this
        // with `alloc_if_needed` when parsing json to allow the json
        // to reference the arena-allocated memory if it can. That way both
        // our temp and perm memory is part of the same arena. This slightly
        // bloats our memory requirements but reduces allocations.
        // PORT NOTE: the arena (+ `errdefer arena.deinit()`) collapses to Rust
        // ownership — every intermediate buffer is freed on all paths.

        // Parse our elements. We do this outside of the struct assignment
        // below to avoid the issue where order matters in struct assignment.
        let headers = Self::parseHeader(reader)?;
        let items = Self::parseItems(reader)?;

        Ok(Envelope { headers, items })
    }

    // fn parseHeader(
    //     alloc: Allocator,
    //     reader: *std.Io.Reader,
    // ) !std.json.ObjectMap {
    fn parseHeader<R: BufRead>(reader: &mut R) -> Result<ObjectMap, Error> {
        // var buf: std.Io.Writer.Allocating = .init(alloc);
        let mut buf: Vec<u8> = Vec::new();
        // _ = try reader.streamDelimiterLimit(
        //     &buf.writer,
        //     '\n',
        //     .limited(1024 * 1024), // 1MB, arbitrary choice
        // );
        let _ = streamDelimiterLimit(reader, &mut buf, b'\n', 1024 * 1024)?;
        // _ = reader.discardDelimiterInclusive('\n') catch |err| switch (err) {
        match discardDelimiterInclusive(reader, b'\n') {
            Ok(_) => {}
            // It's okay if there isn't a trailing newline
            Err(StreamErr::EndOfStream) => {}
            Err(err) => return Err(err.into()),
        }

        // const value = try std.json.parseFromSliceLeaky(
        //     std.json.Value,
        //     alloc,
        //     buf.written(),
        //     .{ .allocate = .alloc_if_needed },
        // );
        let mut value = crate::json::parse_from_slice(&buf)?;

        // return switch (value) {
        //     .object => |map| map,
        //     else => error.EnvelopeMalformedHeaders,
        // };
        // (PORT NOTE: `json::Value` has an iterative `Drop` (UD182), so the
        // map is taken out of it rather than moved.)
        match &mut value {
            crate::json::Value::Object(map) => Ok(std::mem::take(map)),
            _ => Err(Error::EnvelopeMalformedHeaders),
        }
    }

    // fn parseItems(
    //     alloc: Allocator,
    //     reader: *std.Io.Reader,
    // ) !std.ArrayList(Item) {
    fn parseItems<R: BufRead>(reader: &mut R) -> Result<Vec<Item>, Error> {
        // var items: std.ArrayList(Item) = .{};
        // errdefer items.deinit(alloc);
        let mut items: Vec<Item> = Vec::new();
        // while (try parseOneItem(alloc, reader)) |item| {
        //     try items.append(alloc, item);
        // }
        while let Some(item) = Self::parseOneItem(reader)? {
            items.push(item);
        }

        Ok(items)
    }

    // fn parseOneItem(
    //     alloc: Allocator,
    //     reader: *std.Io.Reader,
    // ) !?Item {
    fn parseOneItem<R: BufRead>(reader: &mut R) -> Result<Option<Item>, Error> {
        // Get the next item which must start with a header.
        // var buf: std.Io.Writer.Allocating = .init(alloc);
        let mut buf: Vec<u8> = Vec::new();
        // _ = reader.streamDelimiterLimit(
        //     &buf.writer,
        //     '\n',
        //     .limited(1024 * 1024), // 1MB, arbitrary choice
        // ) catch |err| switch (err) {
        //     error.StreamTooLong => return null,
        //     else => return err,
        // };
        match streamDelimiterLimit(reader, &mut buf, b'\n', 1024 * 1024) {
            Ok(_) => {}
            Err(StreamErr::StreamTooLong) => return Ok(None),
            Err(err) => return Err(err.into()),
        }
        // _ = reader.discardDelimiterInclusive('\n') catch |err| switch (err) {
        match discardDelimiterInclusive(reader, b'\n') {
            Ok(_) => {}
            // It's okay if there isn't a trailing newline
            Err(StreamErr::EndOfStream) => {}
            Err(err) => return Err(err.into()),
        }

        // Parse the header JSON
        let headers: ObjectMap = {
            // const line = std.mem.trim(u8, buf.written(), " \t");
            let line = trim(&buf, b" \t");
            // if (line.len == 0) return null;
            if line.is_empty() {
                return Ok(None);
            }

            let mut value = crate::json::parse_from_slice(line)?;

            // break :headers switch (value) {
            //     .object => |map| map,
            //     else => return error.EnvelopeItemMalformedHeaders,
            // };
            match &mut value {
                crate::json::Value::Object(map) => std::mem::take(map),
                _ => return Err(Error::EnvelopeItemMalformedHeaders),
            }
        };

        // Get the event type
        // const typ: ItemType = if (headers.get("type")) |v| switch (v) {
        //     .string => |str| std.meta.stringToEnum(
        //         ItemType,
        //         str,
        //     ) orelse .unknown,
        //     else => return error.EnvelopeItemTypeMissing,
        // } else return error.EnvelopeItemTypeMissing;
        // (PORT NOTE: the reference's `if |v| switch (v)` nesting is collapsed into
        // one match — clippy::collapsible_match — with identical arms.)
        let typ: ItemType = match headers.get("type") {
            Some(crate::json::Value::String(str)) => {
                ItemType::stringToEnum(str).unwrap_or(ItemType::Unknown)
            }
            Some(_) => return Err(Error::EnvelopeItemTypeMissing),
            None => return Err(Error::EnvelopeItemTypeMissing),
        };

        // Get the payload length. The length is not required. If the length
        // is not specified then it is the next line ending in `\n`.
        // const len_: ?u64 = if (headers.get("length")) |v| switch (v) {
        //     .integer => |int| std.math.cast(
        //         u64,
        //         int,
        //     ) orelse return error.EnvelopeItemLengthMalformed,
        //     else => return error.EnvelopeItemLengthMalformed,
        // } else null;
        let len_: Option<u64> = match headers.get("length") {
            Some(v) => match v {
                // `.integer` is an `i64`; an integer outside it is a
                // `.number_string` and takes the `else` arm (S79-F3).
                crate::json::Value::Integer(int) => {
                    Some(u64::try_from(*int).map_err(|_| Error::EnvelopeItemLengthMalformed)?)
                }
                _ => return Err(Error::EnvelopeItemLengthMalformed),
            },
            None => None,
        };

        // Get the payload
        let payload: Vec<u8> = if let Some(len) = len_ {
            // The payload length is specified so read the exact length.
            let mut payload: Vec<u8> = Vec::new();
            // reader.streamExact(&payload.writer, len) catch |err| switch (err) {
            //     error.EndOfStream => return error.EnvelopeItemPayloadTooShort,
            //     else => return err,
            // };
            match streamExact(reader, &mut payload, len as usize) {
                Ok(()) => {}
                Err(StreamErr::EndOfStream) => return Err(Error::EnvelopeItemPayloadTooShort),
                Err(err) => return Err(err.into()),
            }

            // The next byte must be a newline.
            // if (reader.takeByte()) |byte| {
            //     if (byte != '\n') return error.EnvelopeItemPayloadNoNewline;
            // } else |err| switch (err) {
            //     error.EndOfStream => {},
            //     else => return err,
            // }
            match takeByte(reader) {
                Ok(byte) => {
                    if byte != b'\n' {
                        return Err(Error::EnvelopeItemPayloadNoNewline);
                    }
                }
                Err(StreamErr::EndOfStream) => {}
                Err(err) => return Err(err.into()),
            }

            // break :payload try payload.toOwnedSlice();
            payload
        } else {
            // The payload is the next line ending in `\n`. It is required.
            let mut payload: Vec<u8> = Vec::new();
            // _ = reader.streamDelimiterLimit(
            //     &payload.writer,
            //     '\n',
            //     .limited(1024 * 1024), // 50MB, arbitrary choice
            // ) catch |err| switch (err) {
            //     error.StreamTooLong => return error.EnvelopeItemPayloadTooShort,
            //     else => |v| return v,
            // };
            match streamDelimiterLimit(reader, &mut payload, b'\n', 1024 * 1024) {
                Ok(_) => {}
                Err(StreamErr::StreamTooLong) => return Err(Error::EnvelopeItemPayloadTooShort),
                Err(v) => return Err(v.into()),
            }
            // _ = reader.discardDelimiterInclusive('\n') catch |err| switch (err) {
            match discardDelimiterInclusive(reader, b'\n') {
                Ok(_) => {}
                // It's okay if there isn't a trailing newline
                Err(StreamErr::EndOfStream) => {}
                Err(err) => return Err(err.into()),
            }
            // break :payload try payload.toOwnedSlice();
            payload
        };

        // return .{ .encoded = .{
        //     .headers = headers,
        //     .type = typ,
        //     .payload = payload,
        // } };
        Ok(Some(Item::Encoded(EncodedItem {
            headers,
            type_: typ,
            payload,
        })))
    }

    // pub fn deinit(self: *Envelope) void {
    //     self.arena.deinit();
    // }
    // PORT NOTE: `deinit` is Drop (the owned headers/items free on scope exit).

    // /// The arena allocator associated with this envelope
    // pub fn allocator(self: *Envelope) Allocator {
    //     return self.arena.allocator();
    // }
    // PORT NOTE: the arena-allocator accessor collapses (Rust ownership; callers
    // like `Item.decode(v.allocator())` take no allocator).

    /// Serialize the envelope to the given writer.
    ///
    /// This will convert all decoded items to encoded items and
    /// therefore may allocate.
    // pub fn serialize(
    //     self: *Envelope,
    //     writer: *std.Io.Writer,
    // ) !void {
    pub fn serialize<W: std::io::Write>(&mut self, writer: &mut W) -> Result<(), Error> {
        // Header line first
        // try writer.print("{f}\n", .{std.json.fmt(
        //     std.json.Value{ .object = self.headers },
        //     json_opts,
        // )});
        // (PORT NOTE: `json_opts` == `.whitespace = .minified`; see the
        // `json_opts` note at the bottom.)
        crate::json::stringify_object(&mut *writer, &self.headers)?;
        writer.write_all(b"\n")?;

        // Write each item
        // const alloc = self.allocator();
        for (idx, item) in self.items.iter_mut().enumerate() {
            // if (idx > 0) try writer.writeByte('\n');
            if idx > 0 {
                writer.write_all(b"\n")?;
            }

            // const encoded = try item.encode(alloc);
            // assert(item.* == .encoded);
            // (PORT NOTE: `encode` returns a borrow of the now-`.encoded` item, so
            // the reference's assert is enforced by construction.)
            let encoded = item.encode();

            // try writer.print("{f}\n{s}", .{
            //     std.json.fmt(
            //         std.json.Value{ .object = encoded.headers },
            //         json_opts,
            //     ),
            //     encoded.payload,
            // });
            crate::json::stringify_object(&mut *writer, &encoded.headers)?;
            writer.write_all(b"\n")?;
            writer.write_all(&encoded.payload)?;
        }
        Ok(())
    }
}

/// The various item types that can be in an envelope. This is a point
/// in time snapshot of the types that are known whenever this is edited.
/// Event types can be introduced at any time and unknown types will
/// take the "unknown" enum value.
///
/// https://develop.sentry.dev/sdk/envelopes/#data-model
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    /// Special event type for when the item type is unknown.
    Unknown,

    /// Documented event types
    Event,
    Transaction,
    Attachment,
    Session,
    Sessions,
    Statsd,
    MetricMeta,
    UserFeedback,
    ClientReport,
    ReplayEvent,
    ReplayRecording,
    Profile,
    CheckIn,
}

impl ItemType {
    /// PORT NOTE: `std.meta.stringToEnum(ItemType, str)` — lookup by the reference
    /// enum tag names.
    pub fn stringToEnum(str: &str) -> Option<ItemType> {
        Some(match str {
            "unknown" => ItemType::Unknown,
            "event" => ItemType::Event,
            "transaction" => ItemType::Transaction,
            "attachment" => ItemType::Attachment,
            "session" => ItemType::Session,
            "sessions" => ItemType::Sessions,
            "statsd" => ItemType::Statsd,
            "metric_meta" => ItemType::MetricMeta,
            "user_feedback" => ItemType::UserFeedback,
            "client_report" => ItemType::ClientReport,
            "replay_event" => ItemType::ReplayEvent,
            "replay_recording" => ItemType::ReplayRecording,
            "profile" => ItemType::Profile,
            "check_in" => ItemType::CheckIn,
            _ => return None,
        })
    }
}

/// An item in the envelope. An item can be either in an encoded
/// or decoded state. The encoded state lets us parse the envelope
/// more cheaply since we can defer the full decoding of the item
/// until we need it.
///
/// The decoded state is more ergonomic to work with and lets us
/// easily build up new items and defer encoding until serialization
/// time.
// pub const Item = union(enum) {
#[derive(Debug)]
pub enum Item {
    Encoded(EncodedItem),
    Attachment(Attachment),
}

/// PORT NOTE: `pub const DecodeError = Allocator.Error || error{...}` (nested in
/// `Item` in the reference; hoisted — Rust inherent associated types are
/// unstable). Allocation is infallible in Rust, so only the named cases remain.
#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    MissingRequiredField,
    InvalidFieldType,
    UnsupportedType,
}

impl Item {
    /// Convert the item to an encoded item. This modify the item
    /// in place.
    // pub fn encode(
    //     self: *Item,
    //     alloc: Allocator,
    // ) !EncodedItem {
    // PORT NOTE: returns a borrow of the in-place encoded item (the reference
    // returns the arena-aliased `EncodedItem` by value); the allocator
    // parameter collapses, and with it the `!` (allocation is infallible).
    pub fn encode(&mut self) -> &EncodedItem {
        // const result: EncodedItem = switch (self.*) {
        //     .encoded => |v| return v,
        //     .attachment => |*v| try v.encode(alloc),
        // };
        // self.* = .{ .encoded = result };
        // return result;
        if let Item::Attachment(v) = self {
            let result = v.encode();
            *self = Item::Encoded(result);
        }
        match self {
            Item::Encoded(v) => v,
            Item::Attachment(_) => unreachable!(),
        }
    }

    /// Returns the type of item represented here, whether
    /// it is an encoded item or not.
    pub fn itemType(&self) -> ItemType {
        // return switch (self) {
        //     .encoded => |v| v.type,
        //     .attachment => .attachment,
        // };
        match self {
            Item::Encoded(v) => v.type_,
            Item::Attachment(_) => ItemType::Attachment,
        }
    }

    /// Decode the item if it is encoded. This will modify itself.
    /// If the item is already decoded this does nothing.
    ///
    /// The allocator argument should be an arena-style allocator,
    /// typically the allocator associated with the Envelope.
    ///
    /// If the decoding fails because the item is in an invalid
    /// state (i.e. its missing a required field) then this will
    /// error but the encoded item will remain unmodified. This
    /// allows the caller to handle the error without corrupting the
    /// envelope.
    ///
    /// If decoding fails, the allocator may still allocate so the
    /// allocator should be an arena-style allocator.
    // pub fn decode(self: *Item, alloc: Allocator) DecodeError!void {
    pub fn decode(&mut self) -> Result<(), DecodeError> {
        // Get our encoded item. If we're not encoded we're done.
        // const encoded: EncodedItem = switch (self.*) {
        //     .encoded => |v| v,
        //     else => return,
        // };
        let encoded = match self {
            Item::Encoded(v) => v,
            _ => return Ok(()),
        };

        // Decode the item.
        // self.* = switch (encoded.type) {
        //     .attachment => .{ .attachment = try .decode(alloc, encoded) },
        //     else => return error.UnsupportedType,
        // };
        *self = match encoded.type_ {
            ItemType::Attachment => Item::Attachment(Attachment::decode(encoded)?),
            _ => return Err(DecodeError::UnsupportedType),
        };
        Ok(())
    }
}

/// An encoded item. It is "encoded" in the sense that the payload
/// is a byte slice. The headers are "decoded" into a json ObjectMap
/// but that's still a pretty low-level representation.
#[derive(Debug)]
pub struct EncodedItem {
    pub headers: ObjectMap,
    // PORT NOTE: `type: ItemType` — `type` is a Rust keyword.
    pub type_: ItemType,
    pub payload: Vec<u8>,
}

/// An arbitrary file attachment.
///
/// https://develop.sentry.dev/sdk/envelopes/#attachment
#[derive(Debug, Default)]
pub struct Attachment {
    /// "filename" field is the name of the uploaded file without
    /// a path component.
    pub filename: String,

    /// A special "type" associated with the attachment. This
    /// is documented on the Sentry website. In the future we should
    /// make this an enum.
    // PORT NOTE: `type: ?[]const u8 = null` — `type` is a Rust keyword; the value
    // originates from (and returns to) a JSON string, so `String` is lossless.
    pub type_: Option<String>,

    /// Additional headers for the attachment.
    pub headers_extra: ObjectMapUnmanaged,

    /// The data for the attachment.
    pub payload: Vec<u8>,
}

impl Attachment {
    // pub fn decode(
    //     alloc: Allocator,
    //     item: EncodedItem,
    // ) Item.DecodeError!Attachment {
    //     _ = alloc;
    // PORT NOTE: the arena lets the reference alias `item`'s memory; Rust clones
    // (the encoded item stays untouched on error, per `Item.decode`'s doc).
    pub fn decode(item: &EncodedItem) -> Result<Attachment, DecodeError> {
        Ok(Attachment {
            // .filename = if (item.headers.get("filename")) |v| switch (v) {
            //     .string => |str| str,
            //     else => return error.InvalidFieldType,
            // } else return error.MissingRequiredField,
            filename: match item.headers.get("filename") {
                Some(v) => match v {
                    crate::json::Value::String(str) => str.clone(),
                    _ => return Err(DecodeError::InvalidFieldType),
                },
                None => return Err(DecodeError::MissingRequiredField),
            },

            // .type = if (item.headers.get("attachment_type")) |v| switch (v) {
            //     .string => |str| str,
            //     else => return error.InvalidFieldType,
            // } else null,
            type_: match item.headers.get("attachment_type") {
                Some(v) => match v {
                    crate::json::Value::String(str) => Some(str.clone()),
                    _ => return Err(DecodeError::InvalidFieldType),
                },
                None => None,
            },

            // .headers_extra = item.headers.unmanaged,
            headers_extra: item.headers.clone(),
            // .payload = item.payload,
            payload: item.payload.clone(),
        })
    }

    // pub fn encode(
    //     self: *Attachment,
    //     alloc: Allocator,
    // ) !EncodedItem {
    // PORT NOTE: infallible (allocation); the caller (`Item.encode`) replaces the
    // attachment with the result, so the headers/payload are moved out
    // (`mem::take`) rather than arena-aliased.
    pub fn encode(&mut self) -> EncodedItem {
        // try self.headers_extra.put(
        //     alloc,
        //     "filename",
        //     .{ .string = self.filename },
        // );
        self.headers_extra.insert(
            "filename".to_string(),
            crate::json::Value::String(self.filename.clone()),
        );

        // if (self.type) |v| {
        //     try self.headers_extra.put(
        //         alloc,
        //         "attachment_type",
        //         .{ .string = v },
        //     );
        // } else {
        //     _ = self.headers_extra.swapRemove("attachment_type");
        // }
        if let Some(v) = &self.type_ {
            self.headers_extra.insert(
                "attachment_type".to_string(),
                crate::json::Value::String(v.clone()),
            );
        } else {
            let _ = self.headers_extra.swap_remove("attachment_type");
        }

        // return .{
        //     .headers = self.headers_extra.promote(alloc),
        //     .type = .attachment,
        //     .payload = self.payload,
        // };
        EncodedItem {
            headers: std::mem::take(&mut self.headers_extra),
            type_: ItemType::Attachment,
            payload: std::mem::take(&mut self.payload),
        }
    }
}

/// Same as std.json.ObjectMap but unmanaged. This lets us store
/// them alongside all our items without the overhead of duplicated
/// allocators. Additional, items do not own their own memory so this
/// makes it clear that deinit of an item will not free the memory.
// PORT NOTE: the managed/unmanaged split is a Zig allocator-plumbing distinction
// with no Rust counterpart — both alias the same order-preserving map type.
pub type ObjectMapUnmanaged = ObjectMap;

// /// The options we must use for serialization.
// const json_opts: std.json.Stringify.Options = .{
//     // This is the default but I want to be explicit because its
//     // VERY important for the correctness of the envelope. This is
//     // the only whitespace type in std.json that doesn't emit newlines.
//     // All JSON headers in the envelope must be on a single line.
//     .whitespace = .minified,
// };
// PORT NOTE: `crate::json::stringify_object` is the `.minified` printer (no
// whitespace at all), which is exactly the required option.

// ---------------------------------------------------------------------------
// PORT NOTE: `std.Io.Reader` helpers. Zig 0.15.2 semantics reproduced exactly:
//   * streamDelimiterLimit — appends to `w` until `delimiter` (NOT consumed,
//     NOT written) or EOF (returns bytes streamed); `error.StreamTooLong` if
//     the limit is exhausted before the delimiter.
//   * discardDelimiterInclusive — discards through the delimiter;
//     `error.EndOfStream` if the stream ends first.
//   * streamExact — exactly `n` bytes or `error.EndOfStream`.
//   * takeByte — one byte or `error.EndOfStream`.
// ---------------------------------------------------------------------------

/// PORT NOTE: the Zig stream-error variants used above (`error.EndOfStream`,
/// `error.StreamTooLong`, `error.ReadFailed`).
#[derive(Debug)]
enum StreamErr {
    EndOfStream,
    StreamTooLong,
    ReadFailed(std::io::Error),
}

fn streamDelimiterLimit<R: BufRead>(
    r: &mut R,
    w: &mut Vec<u8>,
    delimiter: u8,
    limit: usize,
) -> Result<usize, StreamErr> {
    let mut remaining = limit;
    while remaining != 0 {
        let available = match r.fill_buf() {
            Ok(buf) => buf,
            Err(err) => return Err(StreamErr::ReadFailed(err)),
        };
        // EndOfStream => return @intFromEnum(limit) - remaining
        if available.is_empty() {
            return Ok(limit - remaining);
        }
        let available = &available[..available.len().min(remaining)];
        if let Some(delimiter_index) = available.iter().position(|&b| b == delimiter) {
            w.extend_from_slice(&available[..delimiter_index]);
            r.consume(delimiter_index);
            remaining -= delimiter_index;
            return Ok(limit - remaining);
        }
        let n = available.len();
        w.extend_from_slice(available);
        r.consume(n);
        remaining -= n;
    }
    Err(StreamErr::StreamTooLong)
}

fn discardDelimiterInclusive<R: BufRead>(r: &mut R, delimiter: u8) -> Result<usize, StreamErr> {
    let mut n: usize = 0;
    loop {
        let available = match r.fill_buf() {
            Ok(buf) => buf,
            Err(err) => return Err(StreamErr::ReadFailed(err)),
        };
        if available.is_empty() {
            return Err(StreamErr::EndOfStream);
        }
        if let Some(delimiter_index) = available.iter().position(|&b| b == delimiter) {
            r.consume(delimiter_index + 1);
            return Ok(n + delimiter_index + 1);
        }
        let len = available.len();
        r.consume(len);
        n += len;
    }
}

fn streamExact<R: BufRead>(r: &mut R, w: &mut Vec<u8>, n: usize) -> Result<(), StreamErr> {
    let mut remaining = n;
    while remaining != 0 {
        let available = match r.fill_buf() {
            Ok(buf) => buf,
            Err(err) => return Err(StreamErr::ReadFailed(err)),
        };
        if available.is_empty() {
            return Err(StreamErr::EndOfStream);
        }
        let take = available.len().min(remaining);
        w.extend_from_slice(&available[..take]);
        r.consume(take);
        remaining -= take;
    }
    Ok(())
}

fn takeByte<R: BufRead>(r: &mut R) -> Result<u8, StreamErr> {
    let available = match r.fill_buf() {
        Ok(buf) => buf,
        Err(err) => return Err(StreamErr::ReadFailed(err)),
    };
    if available.is_empty() {
        return Err(StreamErr::EndOfStream);
    }
    let byte = available[0];
    r.consume(1);
    Ok(byte)
}

/// PORT NOTE: `std.mem.trim(u8, slice, values_to_strip)` — strip leading and
/// trailing bytes that appear in `strip`.
fn trim<'a>(slice: &'a [u8], strip: &[u8]) -> &'a [u8] {
    let mut start = 0;
    while start < slice.len() && strip.contains(&slice[start]) {
        start += 1;
    }
    let mut end = slice.len();
    while end > start && strip.contains(&slice[end - 1]) {
        end -= 1;
    }
    &slice[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "Envelope parse" {
    #[test]
    fn envelope_parse() {
        // const testing = std.testing;
        // const alloc = testing.allocator;

        // var reader: std.Io.Reader = .fixed(
        //     \\{}
        // );
        let mut reader: &[u8] = b"{}";
        let v = Envelope::parse(&mut reader).unwrap();
        // defer v.deinit();
        drop(v);
    }

    // test "Envelope parse session" {
    #[test]
    fn envelope_parse_session() {
        let mut reader: &[u8] = concat!(
            "{}\n",
            "{\"type\":\"session\",\"length\":218}\n",
            "{\"init\":true,\"sid\":\"c148cc2f-5f9f-4231-575c-2e85504d6434\",\"status\":\"abnormal\",\"errors\":0,\"started\":\"2024-08-29T02:38:57.607016Z\",\"duration\":0.000343,\"attrs\":{\"release\":\"0.1.0-HEAD+d37b7d09\",\"environment\":\"production\"}}",
        )
        .as_bytes();
        let v = Envelope::parse(&mut reader).unwrap();
        // defer v.deinit();

        // try testing.expectEqual(@as(usize, 1), v.items.items.len);
        assert_eq!(1usize, v.items.len());
        // try testing.expectEqual(ItemType.session, v.items.items[0].encoded.type);
        let Item::Encoded(encoded) = &v.items[0] else {
            panic!("expected encoded item")
        };
        assert_eq!(ItemType::Session, encoded.type_);
    }

    // test "Envelope parse multiple" {
    #[test]
    fn envelope_parse_multiple() {
        let mut reader: &[u8] = concat!(
            "{}\n",
            "{\"type\":\"session\",\"length\":218}\n",
            "{\"init\":true,\"sid\":\"c148cc2f-5f9f-4231-575c-2e85504d6434\",\"status\":\"abnormal\",\"errors\":0,\"started\":\"2024-08-29T02:38:57.607016Z\",\"duration\":0.000343,\"attrs\":{\"release\":\"0.1.0-HEAD+d37b7d09\",\"environment\":\"production\"}}\n",
            "{\"type\":\"attachment\",\"length\":4,\"filename\":\"test.txt\"}\n",
            "ABCD",
        )
        .as_bytes();
        let v = Envelope::parse(&mut reader).unwrap();
        // defer v.deinit();

        // try testing.expectEqual(@as(usize, 2), v.items.items.len);
        assert_eq!(2usize, v.items.len());
        // try testing.expectEqual(ItemType.session, v.items.items[0].encoded.type);
        let Item::Encoded(encoded) = &v.items[0] else {
            panic!("expected encoded item")
        };
        assert_eq!(ItemType::Session, encoded.type_);
        // try testing.expectEqual(ItemType.attachment, v.items.items[1].encoded.type);
        let Item::Encoded(encoded) = &v.items[1] else {
            panic!("expected encoded item")
        };
        assert_eq!(ItemType::Attachment, encoded.type_);
    }

    // test "Envelope parse multiple no length" {
    #[test]
    fn envelope_parse_multiple_no_length() {
        let mut reader: &[u8] = concat!(
            "{}\n",
            "{\"type\":\"session\"}\n",
            "{}\n",
            "{\"type\":\"attachment\",\"length\":4,\"filename\":\"test.txt\"}\n",
            "ABCD",
        )
        .as_bytes();
        let v = Envelope::parse(&mut reader).unwrap();
        // defer v.deinit();

        // try testing.expectEqual(@as(usize, 2), v.items.items.len);
        assert_eq!(2usize, v.items.len());
        // try testing.expectEqual(ItemType.session, v.items.items[0].encoded.type);
        let Item::Encoded(encoded) = &v.items[0] else {
            panic!("expected encoded item")
        };
        assert_eq!(ItemType::Session, encoded.type_);
        // try testing.expectEqual(ItemType.attachment, v.items.items[1].encoded.type);
        let Item::Encoded(encoded) = &v.items[1] else {
            panic!("expected encoded item")
        };
        assert_eq!(ItemType::Attachment, encoded.type_);
    }

    // test "Envelope parse end in new line" {
    #[test]
    fn envelope_parse_end_in_new_line() {
        let mut reader: &[u8] = concat!(
            "{}\n",
            "{\"type\":\"session\",\"length\":218}\n",
            "{\"init\":true,\"sid\":\"c148cc2f-5f9f-4231-575c-2e85504d6434\",\"status\":\"abnormal\",\"errors\":0,\"started\":\"2024-08-29T02:38:57.607016Z\",\"duration\":0.000343,\"attrs\":{\"release\":\"0.1.0-HEAD+d37b7d09\",\"environment\":\"production\"}}\n",
        )
        .as_bytes();
        let v = Envelope::parse(&mut reader).unwrap();
        // defer v.deinit();

        // try testing.expectEqual(@as(usize, 1), v.items.items.len);
        assert_eq!(1usize, v.items.len());
        // try testing.expectEqual(ItemType.session, v.items.items[0].encoded.type);
        let Item::Encoded(encoded) = &v.items[0] else {
            panic!("expected encoded item")
        };
        assert_eq!(ItemType::Session, encoded.type_);
    }

    // test "Envelope parse attachment" {
    #[test]
    fn envelope_parse_attachment() {
        let mut reader: &[u8] = concat!(
            "{}\n",
            "{\"type\":\"attachment\",\"length\":4,\"filename\":\"test.txt\"}\n",
            "ABCD",
        )
        .as_bytes();
        let mut v = Envelope::parse(&mut reader).unwrap();
        // defer v.deinit();

        // try testing.expectEqual(@as(usize, 1), v.items.items.len);
        assert_eq!(1usize, v.items.len());

        {
            // var item = &v.items.items[0];
            let item = &mut v.items[0];
            // try testing.expectEqual(ItemType.attachment, item.encoded.type);
            let Item::Encoded(encoded) = &*item else {
                panic!("expected encoded item")
            };
            assert_eq!(ItemType::Attachment, encoded.type_);
            // try item.decode(v.allocator());
            item.decode().unwrap();
            // try testing.expect(item.* == .attachment);
            assert!(matches!(item, Item::Attachment(_)));
            // try testing.expectEqualStrings("test.txt", item.attachment.filename);
            let Item::Attachment(attachment) = &*item else {
                unreachable!()
            };
            assert_eq!("test.txt", attachment.filename);
        }

        // Serialization test
        {
            // var output: std.Io.Writer.Allocating = .init(alloc);
            // defer output.deinit();
            let mut output: Vec<u8> = Vec::new();
            v.serialize(&mut output).unwrap();
            assert_eq!(
                concat!(
                    "{}\n",
                    "{\"type\":\"attachment\",\"length\":4,\"filename\":\"test.txt\"}\n",
                    "ABCD",
                ),
                // std.mem.trim(u8, output.written(), "\n")
                String::from_utf8(output).unwrap().trim_matches('\n'),
            );
        }
    }

    // test "Envelope serialize empty" {
    #[test]
    fn envelope_serialize_empty() {
        let mut reader: &[u8] = b"{}";
        let mut v = Envelope::parse(&mut reader).unwrap();
        // defer v.deinit();

        // var output: std.Io.Writer.Allocating = .init(alloc);
        // defer output.deinit();
        let mut output: Vec<u8> = Vec::new();
        v.serialize(&mut output).unwrap();

        assert_eq!(
            "{}",
            // std.mem.trim(u8, output.written(), "\n")
            String::from_utf8(output).unwrap().trim_matches('\n'),
        );
    }

    // test "Envelope serialize session" {
    #[test]
    fn envelope_serialize_session() {
        let mut reader: &[u8] = concat!(
            "{}\n",
            "{\"type\":\"session\",\"length\":218}\n",
            "{\"init\":true,\"sid\":\"c148cc2f-5f9f-4231-575c-2e85504d6434\",\"status\":\"abnormal\",\"errors\":0,\"started\":\"2024-08-29T02:38:57.607016Z\",\"duration\":0.000343,\"attrs\":{\"release\":\"0.1.0-HEAD+d37b7d09\",\"environment\":\"production\"}}",
        )
        .as_bytes();
        let mut v = Envelope::parse(&mut reader).unwrap();
        // defer v.deinit();

        // var output: std.Io.Writer.Allocating = .init(alloc);
        // defer output.deinit();
        let mut output: Vec<u8> = Vec::new();
        v.serialize(&mut output).unwrap();

        assert_eq!(
            concat!(
                "{}\n",
                "{\"type\":\"session\",\"length\":218}\n",
                "{\"init\":true,\"sid\":\"c148cc2f-5f9f-4231-575c-2e85504d6434\",\"status\":\"abnormal\",\"errors\":0,\"started\":\"2024-08-29T02:38:57.607016Z\",\"duration\":0.000343,\"attrs\":{\"release\":\"0.1.0-HEAD+d37b7d09\",\"environment\":\"production\"}}",
            ),
            // std.mem.trim(u8, output.written(), "\n")
            String::from_utf8(output).unwrap().trim_matches('\n'),
        );
    }

    fn parse_err(reader: &mut &[u8]) -> Error {
        match Envelope::parse(reader) {
            Ok(_) => panic!("expected the envelope to be rejected"),
            Err(e) => e,
        }
    }

    // No reference test. LEDGER S79-F3: the headers are parsed with
    // `std.json.parseFromSliceLeaky(std.json.Value, ..)`, whose defaults reject
    // a duplicate key (`error.DuplicateField`) and keep an integer outside
    // `i64` as `.number_string`, which the `length` switch rejects
    // (`error.EnvelopeItemLengthMalformed`). MEASURED on zig 0.16.0 (the
    // golden file in `json.rs`).
    #[test]
    fn header_parse_follows_std_json_edge_cases() {
        // A duplicate key in the envelope header, and in an item header.
        let mut reader: &[u8] = b"{\"a\":1,\"a\":2}\n";
        let err = parse_err(&mut reader);
        assert!(format!("{err:?}").contains("DuplicateField"), "{err:?}");
        let mut reader: &[u8] = b"{}\n{\"type\":\"session\",\"type\":\"event\"}\n{}\n";
        let err = parse_err(&mut reader);
        assert!(format!("{err:?}").contains("DuplicateField"), "{err:?}");

        // `length` just past `i64::MAX`, and past `u64::MAX`: both are
        // `.number_string`, so the length is malformed (not a short payload).
        for len in ["9223372036854775808", "18446744073709551616"] {
            let input = format!("{{}}\n{{\"type\":\"session\",\"length\":{len}}}\nx");
            let mut reader: &[u8] = input.as_bytes();
            let err = parse_err(&mut reader);
            assert!(
                matches!(err, Error::EnvelopeItemLengthMalformed),
                "length {len}: {err:?}"
            );
        }

        // Control: an in-range length still parses.
        let mut reader: &[u8] = b"{}\n{\"type\":\"session\",\"length\":1}\nx";
        let v = Envelope::parse(&mut reader).unwrap();
        assert_eq!(1, v.items.len());
    }

    // No reference test. LEDGER S79-F4: headers are written with
    // `std.json.fmt(.., .{ .whitespace = .minified })`, whose number printing
    // differs from serde_json's (MEASURED on zig 0.16.0: `2.0` -> `2`, `-0.0`
    // -> `-0`, `1e2` -> `100`, `1e400` and `18446744073709551616` verbatim).
    #[test]
    fn header_serialize_prints_numbers_like_std_json() {
        let mut reader: &[u8] = concat!(
            "{\"f\":2.0,\"z\":-0.0,\"e\":1e2,\"big\":18446744073709551616,\"inf\":1e400}\n",
            "{\"type\":\"session\",\"length\":2,\"x\":1.50}\n",
            "{}",
        )
        .as_bytes();
        let mut v = Envelope::parse(&mut reader).unwrap();
        let mut output: Vec<u8> = Vec::new();
        v.serialize(&mut output).unwrap();
        assert_eq!(
            concat!(
                "{\"f\":2,\"z\":-0,\"e\":100,\"big\":18446744073709551616,\"inf\":1e400}\n",
                "{\"type\":\"session\",\"length\":2,\"x\":1.5}\n",
                "{}",
            ),
            String::from_utf8(output).unwrap(),
        );
    }

    // No reference test. Reference defect (R13), UD182: `std.json.fmt` panics
    // on a header nested deeper than 256 levels (MEASURED on zig 0.16.0
    // ReleaseSafe, exit 134), while `parseFromSliceLeaky` accepts it. The port
    // parses it too, and `serialize` returns an `InvalidInput` error instead
    // of panicking. Negative control: drop the depth check in
    // `json::stringify_object` and this fails on `unwrap_err`.
    #[test]
    fn serialize_rejects_a_header_deeper_than_256_levels() {
        let header = |depth: usize| {
            // The header object is itself one level.
            let inner = depth - 1;
            format!("{{\"x\":{}0{}}}\n", "[".repeat(inner), "]".repeat(inner))
        };

        // 256 levels: parses and round-trips.
        let input = header(256);
        let mut reader: &[u8] = input.as_bytes();
        let mut v = Envelope::parse(&mut reader).unwrap();
        let mut output: Vec<u8> = Vec::new();
        v.serialize(&mut output).unwrap();
        assert_eq!(input, String::from_utf8(output).unwrap());

        // 257 levels: parses (as the reference does), serialize errors.
        let input = header(257);
        let mut reader: &[u8] = input.as_bytes();
        let mut v = Envelope::parse(&mut reader).unwrap();
        assert!(v.headers.contains_key("x"));
        let mut output: Vec<u8> = Vec::new();
        let err = v.serialize(&mut output).unwrap_err();
        assert!(
            matches!(&err, Error::Io(e) if e.kind() == std::io::ErrorKind::InvalidInput),
            "{err:?}"
        );
        assert!(output.is_empty(), "nothing is written before the error");
    }
}
