//! rustty-font opentype glyf
//!
//! PORTS: ghostty src/font/opentype/glyf.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const Allocator = std.mem.Allocator;
//   const sfnt = @import("sfnt.zig");
// `Allocator` is not threaded through: `Outline` owns its `contours`/`points` as
// `Vec`s (freed on drop), so `decode` takes no allocator and `Outline.deinit` has
// no port-side equivalent. The big-endian on-disk reads the reference does via
// `std.io.fixedBufferStream` + `reader.readInt(..., .big)` / `readByte` /
// `skipBytes` are ported as an explicit cursor over the byte slice.
use crate::opentype::sfnt;

/// Glyph Data Table
///
/// This takes a little bit of a different form than other tables that we
/// have parsers for. Due to the fact that this table contains arrays of
/// arbitrary length, we store a pointer (slice) to the underlying data,
/// and then have functions for getting and interpreting specific parts.
///
/// References:
/// - https://learn.microsoft.com/en-us/typography/opentype/spec/glyf
///
/// Field names are in camelCase to match names in spec.
pub struct Glyf<'a> {
    pub data: &'a [u8],
}

/// A decoded glyph outline.
///
/// The `contours` slice is the list of end point indices and
/// `points` owns all the points. Glyf guarantees that contour
/// points are sequential so we can just store the end and calculate
/// the points that way. Use the helpers to make it ergonomic.
// PORT NOTE: the reference `Outline` is `pub const Outline = struct { ... }`
// nested in `Glyf`; Rust has no nested type declarations, so it is hoisted to a
// sibling type (referenced as `Glyf.Outline` in Zig, `Outline` here). Its
// `contours`/`points` are reference allocator-owned slices; here they are owned
// `Vec`s, so `deinit` is dropped (handled by `Drop`).
pub struct Outline {
    /// List of contour end points. Calculate the full list of
    /// points using points[prev...this+1]
    pub contours: Vec<sfnt::Uint16>,

    /// The backing storage of all points in the entry.
    pub points: Vec<Point>,
}

/// A single decoded point in a simple glyph contour.
// PORT NOTE: the reference `Point` is nested in `Outline`; hoisted to a sibling
// type (referenced as `Glyf.Outline.Point` in Zig, `Point` here).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
    pub on_curve: bool,
}

impl Outline {
    /// Return the point slice for the contour at `index`.
    ///
    /// The returned slice references `points` and is invalidated when
    /// this outline is deinitialized.
    pub fn contour(&self, index: usize) -> &[Point] {
        let start = if index == 0 {
            0
        } else {
            self.contours[index - 1] as usize + 1
        };
        let end = self.contours[index] as usize + 1;
        &self.points[start..end]
    }

    // PORT NOTE: the reference declares
    //   /// Free all memory owned by this outline. Pass in the same
    //   /// allocator used for decoding.
    //   pub fn deinit(self: *Outline, alloc: Allocator) void
    // which frees `contours`/`points` and sets `self.* = undefined`. In the Rust
    // port these are owned `Vec`s freed automatically on drop, so there is no
    // `deinit`.
}

/// https://learn.microsoft.com/en-us/typography/opentype/spec/glyf#table-organization
// PORT NOTE: the reference `Entry` is `pub const Entry = struct { ... }` nested
// in `Glyf`; hoisted to a sibling type (referenced as `Glyf.Entry` in Zig).
pub struct Entry<'a> {
    pub header: Header,

    /// We store a reference to the original bytes so that we can
    /// validate or iterate the contours or components of the glyph.
    ///
    /// This data starts immediately after the header.
    pub data: &'a [u8],
}

/// The header that's always present at
/// the start of any glyph in the table.
///
/// Depending on the number of contours, the data that
/// comes afterwards must be interpreted differently.
///
/// References:
/// - https://learn.microsoft.com/en-us/typography/opentype/spec/glyf#glyph-headers
// PORT NOTE: the reference `Header` is an `extern struct` with `align(1)`
// big-endian fields nested in `Entry`; hoisted to a sibling type and parsed
// explicitly big-endian.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct Header {
    /// If the number of contours is greater than
    /// or equal to zero, this is a simple glyph.
    ///
    /// If negative, this is a composite glyph — the
    /// value -1 should be used for composite glyphs.
    pub numberOfContours: sfnt::Int16,

    /// Minimum x for coordinate data.
    pub xMin: sfnt::Int16,

    /// Minimum y for coordinate data.
    pub yMin: sfnt::Int16,

    /// Maximum x for coordinate data.
    pub xMax: sfnt::Int16,

    /// Maximum y for coordinate data.
    pub yMax: sfnt::Int16,
}

/// The size, in bytes, of [`Header`] as laid out on disk.
// PORT NOTE: the reference uses `@sizeOf(Header)` of an `extern struct` of five
// `int16 align(1)` fields, i.e. 10 bytes. We name that constant explicitly since
// the Rust `Header` struct's `size_of` is not the on-disk size.
pub const HEADER_SIZE: usize = 10;

/// The bit flags that describe the point data in simple glyph entries.
///
/// Doc strings for each field are copied with minimal modification
/// from the opentype spec. Field names are altered to be clearer and
/// more succinct, and mentions of those field names in doc strings
/// have been similarly modified to match the ones in the struct.
///
/// The relationship between the <x|y>_short and <x|y>_repeat_or_sign
/// fields is important, and poorly explained in prose, so instead of
/// that, here's a table that should make it easier to understand.
///
///          x_short > | false            | true             |
/// x_repeat_or_sign V |------------------|------------------|
///                    | The x-coordinate | The x-coordinate |
///              false | of this point is | of this point is |
///                    | a signed 16-bit  | an unsigned byte |
///                    | value added to   | value treated as |
///                    | the *Coordinates | negative, added  |
///                    | array.           | to the array of  |
///                    |                  | xCoordinates.    |
/// -------------------|------------------|------------------|
///                    | The x-coordinate | The x-coordinate |
///               true | of this point is | of this point is |
///                    | the same as the  | an unsigned byte |
///                    | previous point;  | value treated as |
///                    | nothing added to | positive, added  |
///                    | the xCoordinates | to the array of  |
///                    | array.           | xCoordinates.    |
/// -------------------|------------------|------------------|
///
/// References:
/// - https://learn.microsoft.com/en-us/typography/opentype/spec/glyf#simple-glyph-description
// PORT NOTE: the reference `SimpleFlags` is a `packed struct(u8)` whose fields
// are single bits laid out low-to-high in the order declared. Rust has no
// bitfield structs, so this is a `#[repr(transparent)]` newtype over the raw
// `u8` with one accessor per field reading the corresponding bit (bit 0 =
// `on_curve`, bit 1 = `x_short`, ... bit 7 = `reserved`). `@bitCast(byte)` maps
// to `SimpleFlags(byte)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SimpleFlags(pub u8);

impl SimpleFlags {
    /// If set, the point is on the curve; otherwise, it is off the curve.
    pub fn on_curve(self) -> bool {
        self.0 & 0x01 != 0
    }

    /// If set, the corresponding x-coordinate is 1 byte long,
    /// and the sign is determined by the x_repeat_or_sign flag.
    ///
    /// If not set, its interpretation depends on the x_repeat_or_sign
    /// flag: If that other flag is set, the x-coordinate is the same
    /// as the previous x-coordinate, and no element is added to the
    /// xCoordinates array. If both flags are not set, the corresponding
    /// element in the xCoordinates array is two bytes and interpreted
    /// as a signed integer.
    ///
    /// See the description of the x_repeat_or_sign flag for additional
    /// information.
    pub fn x_short(self) -> bool {
        self.0 & 0x02 != 0
    }

    /// If set, the corresponding y-coordinate is 1 byte long,
    /// and the sign is determined by the y_repeat_or_sign flag.
    ///
    /// If not set, its interpretation depends on the y_repeat_or_sign
    /// flag: If that other flag is set, the y-coordinate is the same
    /// as the previous y-coordinate, and no element is added to the
    /// yCoordinates array. If both flags are not set, the corresponding
    /// element in the yCoordinates array is two bytes and interpreted
    /// as a signed integer.
    ///
    /// See the description of the y_repeat_or_sign flag for additional
    /// information.
    pub fn y_short(self) -> bool {
        self.0 & 0x04 != 0
    }

    /// If set, the next byte (read as unsigned) specifies the number
    /// of additional times this flag byte is to be repeated in the
    /// logical flags array — that is, the number of additional logical
    /// flag entries inserted after this entry. (In the expanded logical
    /// array, this bit is ignored.) In this way, the number of flags
    /// listed can be smaller than the number of points in the glyph
    /// description.
    pub fn repeat(self) -> bool {
        self.0 & 0x08 != 0
    }

    /// This flag has two meanings, depending on how the x_short flag
    /// is set. If x_short is set, this bit describes the sign of the
    /// value, with 1 equaling positive and 0 negative. If x_short is
    /// not set and this bit is set, then the current x-coordinate is
    /// the same as the previous x-coordinate. If x_short is not set
    /// and this bit is also not set, the current x-coordinate is a
    /// signed 16-bit delta vector.
    pub fn x_repeat_or_sign(self) -> bool {
        self.0 & 0x10 != 0
    }

    /// This flag has two meanings, depending on how the y_short flag
    /// is set. If y_short is set, this bit describes the sign of the
    /// value, with 1 equaling positive and 0 negative. If y_short is
    /// not set and this bit is set, then the current y-coordinate is
    /// the same as the previous y-coordinate. If y_short is not set
    /// and this bit is also not set, the current y-coordinate is a
    /// signed 16-bit delta vector.
    pub fn y_repeat_or_sign(self) -> bool {
        self.0 & 0x20 != 0
    }

    /// If set, contours in the glyph description could overlap.
    ///
    /// Use of this flag is not required — that is, contours may
    /// overlap without having this flag set. When used, it must
    /// be set on the first flag byte for the glyph.
    pub fn overlap(self) -> bool {
        self.0 & 0x40 != 0
    }

    /// Bit 7 is reserved: set to zero.
    pub fn reserved(self) -> bool {
        self.0 & 0x80 != 0
    }

    /// Determine the size (in bytes) of the corresponding
    /// value in the `xCoordinates` array for this flagset.
    ///
    /// See doc comments on the struct for an explanation.
    // PORT NOTE: the reference returns `u2`; Rust has no `u2`, so this returns
    // `usize` (it is only ever summed into byte tallies).
    #[inline]
    pub fn x_bytes(self) -> usize {
        if self.x_short() {
            // short, 1 byte
            1
        } else if self.x_repeat_or_sign() {
            // repeat, 0 bytes
            0
        } else {
            // otherwise, 16-bit, 2 bytes.
            2
        }
    }

    /// Determine the size (in bytes) of the corresponding
    /// value in the `yCoordinates` array for this flagset.
    ///
    /// See doc comments on the struct for an explanation.
    #[inline]
    pub fn y_bytes(self) -> usize {
        if self.y_short() {
            // short, 1 byte
            1
        } else if self.y_repeat_or_sign() {
            // repeat, 0 bytes
            0
        } else {
            // otherwise, 16-bit, 2 bytes.
            2
        }
    }
}

/// The kind of glyph an [`Entry`] describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    /// A glyph made of standard contours.
    Simple,
    /// A glyph made of references to other glyphs.
    Composite,
}

/// Errors that can be returned from `Entry.size()`.
// PORT NOTE: the reference `SizeError = error{ ... }`. Modeled as a Rust enum so
// `size`/`decode` can return the same distinct error values, matched in tests
// against `Glyf.Entry.SizeError.<Variant>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeError {
    /// The entry's data wasn't large enough, ran
    /// out of bytes before we were done reading.
    EndOfStream,

    /// The entry contains hinting instructions,
    /// which we don't currently support.
    InstructionsNotSupported,

    /// The entry is a composite glyph,
    /// which we don't currently support.
    CompositeNotSupported,

    /// The elements of the end points array
    /// must strictly monotonically increase.
    ///
    /// This error means the provided entry violated that.
    EndPointsOutOfOrder,

    /// This entry defines points past the index determined
    /// by the final element of the endPtsOfContours array.
    TooManyPoints,
}

/// Errors that can be returned from `Entry.decode()`.
// PORT NOTE: the reference
//   DecodeError = SizeError || Allocator.Error || error{CoordinateOverflow}
// `Allocator.Error` has no port-side equivalent (we use `Vec`); the remaining
// members are `SizeError`'s plus `CoordinateOverflow`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The entry's data wasn't large enough, ran
    /// out of bytes before we were done reading.
    EndOfStream,

    /// The entry contains hinting instructions,
    /// which we don't currently support.
    InstructionsNotSupported,

    /// The entry is a composite glyph,
    /// which we don't currently support.
    CompositeNotSupported,

    /// The elements of the end points array
    /// must strictly monotonically increase.
    EndPointsOutOfOrder,

    /// This entry defines points past the index determined
    /// by the final element of the endPtsOfContours array.
    TooManyPoints,

    /// Coordinate delta accumulation overflowed.
    CoordinateOverflow,

    /// Allocation failure while decoding.
    // PORT NOTE (433b16bcb): the reference `Allocator.Error` member. In this
    // port real OOM aborts, so the only producer is `decode_limited`'s
    // per-allocation limit (the `LimitedAllocator` seam).
    OutOfMemory,
}

// PORT NOTE: the reference's `DecodeError` is a superset of `SizeError` via the
// `||` error-set union, so a `SizeError` flows transparently into a `DecodeError`
// at the `try` sites. We make that explicit with a `From` conversion.
impl From<SizeError> for DecodeError {
    fn from(e: SizeError) -> Self {
        match e {
            SizeError::EndOfStream => DecodeError::EndOfStream,
            SizeError::InstructionsNotSupported => DecodeError::InstructionsNotSupported,
            SizeError::CompositeNotSupported => DecodeError::CompositeNotSupported,
            SizeError::EndPointsOutOfOrder => DecodeError::EndPointsOutOfOrder,
            SizeError::TooManyPoints => DecodeError::TooManyPoints,
        }
    }
}

/// Error returned from [`Entry::init`].
// PORT NOTE: the reference `init` returns `error{EndOfStream}!Entry`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryInitError {
    /// Ran out of bytes before the header could be read.
    EndOfStream,
}

impl<'a> Entry<'a> {
    /// Initialize an entry from the provided data.
    ///
    /// This DOES NOT COPY the data, it only stores a pointer to it.
    ///
    /// The lifetime of this struct, then, is the same as the
    /// lifetime of the data that is used to initialize it.
    pub fn init(data: &'a [u8]) -> Result<Entry<'a>, EntryInitError> {
        // PORT NOTE: `reader.readStructEndian(Header, .big)` reads the 10-byte
        // big-endian header off a `fixedBufferStream`, then `data[fbs.pos..]` is
        // the remainder. We parse the header explicitly and slice the rest.
        let mut pos: usize = 0;
        let header = Header {
            numberOfContours: read_be_i16_init(data, &mut pos)?,
            xMin: read_be_i16_init(data, &mut pos)?,
            yMin: read_be_i16_init(data, &mut pos)?,
            xMax: read_be_i16_init(data, &mut pos)?,
            yMax: read_be_i16_init(data, &mut pos)?,
        };
        Ok(Entry {
            header,
            data: &data[pos..],
        })
    }

    /// Identifies what type (simple or composite) of entry this is.
    pub fn entry_type(&self) -> Type {
        if self.header.numberOfContours >= 0 {
            Type::Simple
        } else {
            Type::Composite
        }
    }

    /// Determines the size (in bytes) of this entry.
    ///
    /// If the entry is valid, returns the number of bytes
    /// taken up by this entry, including its header.
    ///
    /// NOTE: Currently produces errors when given composite glyphs
    ///       or any glyphs that have hinting instructions included.
    pub fn size(&self) -> Result<usize, SizeError> {
        // PORT NOTE: the reference opens a `fixedBufferStream(self.data)` and
        // reads through it; `fbs.pos` at the end is the number of bytes consumed
        // after the header. We use an explicit cursor `pos` over `self.data` for
        // the same accounting, returning `EndOfStream` on underflow.
        let mut pos: usize = 0;
        match self.entry_type() {
            // https://learn.microsoft.com/en-us/typography/opentype/spec/glyf#simple-glyph-description
            Type::Simple => {
                let num_contours: usize = self.header.numberOfContours as usize;

                // From the spec:
                //
                // > If a glyph has zero contours, no additional glyph
                // > data beyond the header is required. A glyph with
                // > zero contours may have additional data, however;
                // > in particular, it may have instructions that
                // > operate on phantom points.
                //
                // If our number of contours is 0, and there's less than
                // two bytes in the remaining data, then we just return
                // the size of the header as our size. The reason for
                // two bytes is because that's the minimum size of the
                // extra data, since `instructionLength` is 16 bits.
                if num_contours == 0 && self.data.len() < 2 {
                    return Ok(HEADER_SIZE);
                }

                // uint16 endPtsOfContours[numberOfContours]
                //
                // Array of point indices for the last point
                // of each contour, in increasing numeric order.
                let mut max_point_index: isize = -1;
                for _ in 0..num_contours {
                    let index = read_be_u16_size(self.data, &mut pos)?;
                    // The endpoints are supposed to monotonically increase.
                    if (index as isize) <= max_point_index {
                        return Err(SizeError::EndPointsOutOfOrder);
                    }
                    max_point_index = index as isize;
                }

                // uint16 instructionLength
                //
                // Total number of bytes for instructions.
                //
                // If instructionLength is zero, no instructions
                // are present for this glyph, and this field is
                // followed directly by the flags field.
                let instructions_length = read_be_u16_size(self.data, &mut pos)?;

                // Since we don't have code that validates instruction
                // byte code, we just reject all glyphs that contain any.
                //
                // In the future we could change this to just ignore the
                // instructions, or even validate them, but for now this
                // is fine, since we only need this function at all to
                // validate glyf entries from the glyph protocol, which
                // explicitly forbids instructions anyway.
                if instructions_length > 0 {
                    return Err(SizeError::InstructionsNotSupported);
                }

                // uint8 flags[variable]
                //
                // Array of flag elements.
                //
                // ---
                //
                // We do additional accounting here to figure out how many
                // bytes the next two fields (the [x|y]Coordinates arrays)
                // should take, so that we can just try to throw out that
                // many bytes in order to validate them. This is because
                // the length of each one depends on the flags.
                //
                // We're using `i` here to count the number of logical
                // entries we have, which should reach the number of
                // points defined by the final endpoint (from earlier).
                let mut i: usize = 0;
                let mut x_coords_len: usize = 0;
                let mut y_coords_len: usize = 0;
                while i as isize <= max_point_index {
                    let flag = SimpleFlags(read_byte_size(self.data, &mut pos)?);

                    // Determine how many bytes the x and y coordinates will
                    // be represented with in the corresponding arrays, add
                    // them to our tallies.
                    x_coords_len += flag.x_bytes();
                    y_coords_len += flag.y_bytes();

                    // 0x08 REPEAT_FLAG
                    // Bit 3: If set, the next byte (read as unsigned)
                    // specifies the number of additional times this flag
                    // byte is to be repeated in the logical flags array
                    // — that is, the number of additional logical flag
                    // entries inserted after this entry.
                    if flag.repeat() {
                        // The flag is repeated a certain number of times,
                        // which means that the point count is increased by
                        // that count, and the x_coords_len and y_coords_len
                        // must be increased by the correct number of bytes
                        // as well.
                        let repeat_count: usize = read_byte_size(self.data, &mut pos)? as usize;
                        i += repeat_count;
                        x_coords_len += repeat_count * flag.x_bytes();
                        y_coords_len += repeat_count * flag.y_bytes();

                        // If the repeat count pushes our logical point
                        // number beyond the max point index which we
                        // figured out earlier from the end points, then
                        // there's an issue with this entry, error out.
                        if i as isize > max_point_index {
                            return Err(SizeError::TooManyPoints);
                        }
                    }

                    i += 1;
                }

                // uint8 or int16 xCoordinates[variable]
                //
                // Contour point x-coordinates.
                //
                // ---
                //
                // We determined the length of this section (in bytes)
                // above while processing the flags, so that we can just
                // skip that many bytes to validate this field.
                skip_bytes_size(self.data, &mut pos, x_coords_len)?;

                // uint8 or int16 yCoordinates[variable]
                //
                // Contour point y-coordinates.
                //
                // ---
                //
                // We determined the length of this section (in bytes)
                // above while processing the flags, so that we can just
                // skip that many bytes to validate this field.
                skip_bytes_size(self.data, &mut pos, y_coords_len)?;
            }

            Type::Composite => {
                // We don't have code for validating composite glyphs,
                // mainly because we don't need it, since we only use
                // this function for the glyph protocol which explicitly
                // forbids composite glyphs anyway.
                //
                // So we return false for composite glyphs.
                return Err(SizeError::CompositeNotSupported);
            }
        }

        // No issues found, the glyf entry is valid, return its length.
        Ok(HEADER_SIZE + pos)
    }

    /// Decode this simple glyph entry into an owned outline.
    ///
    /// NOTE: Currently produces errors when given composite glyphs
    ///       or any glyphs that have hinting instructions included.
    // PORT NOTE: the reference signature is
    //   pub fn decode(self: Entry, alloc: Allocator) DecodeError!Glyf.Outline
    // The allocator is dropped (the outline owns `Vec`s). The reference's
    // `errdefer alloc.free(...)` cleanup on the partial allocations is implicit:
    // a `Vec` dropped by the `?`/early return frees itself.
    pub fn decode(&self) -> Result<Outline, DecodeError> {
        self.decode_limited(usize::MAX)
    }

    /// `decode` through a per-allocation byte limit.
    ///
    /// PORT NOTE (433b16bcb): the reference expresses the limit by threading a
    /// `datastruct.LimitedAllocator` through `decode(alloc)` — an allocator
    /// that rejects any single allocation larger than `limit` bytes. This
    /// port's decode owns its `Vec`s (no allocator parameter), so the limit is
    /// threaded explicitly and each allocation site checks its request size,
    /// failing with `DecodeError::OutOfMemory` exactly where the reference's
    /// limited allocator would return null.
    pub fn decode_limited(&self, max_allocation_bytes: usize) -> Result<Outline, DecodeError> {
        // We only support simple glyphs.
        match self.entry_type() {
            Type::Simple => {}
            Type::Composite => return Err(DecodeError::CompositeNotSupported),
        }

        let mut pos: usize = 0;

        // A zero-contour glyph may be header-only. See size for the
        // reason for the hardcoded 2 here.
        let num_contours: usize = self.header.numberOfContours as usize;
        if num_contours == 0 && self.data.len() < 2 {
            return Ok(Outline {
                points: Vec::new(),
                contours: Vec::new(),
            });
        }

        // We now know our full amount of contour ending points.
        // PORT NOTE: `alloc.alloc(uint16, num_contours)` + `errdefer free`;
        // the request goes through the (possible) per-allocation limit.
        if num_contours.saturating_mul(core::mem::size_of::<sfnt::Uint16>()) > max_allocation_bytes
        {
            return Err(DecodeError::OutOfMemory);
        }
        let mut end_points: Vec<sfnt::Uint16> = vec![0; num_contours];

        // If we have no contours, then the only possible remaining
        // field is instructionLength. Instructions are not supported.
        if num_contours == 0 {
            let instructions_length = read_be_u16_decode(self.data, &mut pos)?;
            if instructions_length > 0 {
                return Err(DecodeError::InstructionsNotSupported);
            }
            return Ok(Outline {
                points: Vec::new(),
                contours: end_points,
            });
        }

        // The number of points is determined by the final end point
        // entry since the entries have to be monotonic (something
        // we verify below).
        let point_count: usize = {
            let mut prev_end_point: isize = -1;

            // Go through the end points array and update our end_points
            // with the valid index. The final endpoint tells us our point
            // count, since endpoints are stored as inclusive point indices.
            // PORT NOTE: the reference is `for (0..end_points.len()) |i|`, an
            // index loop; kept as an index loop for parity (clippy's iterator
            // rewrite would obscure the verbatim structure).
            #[allow(clippy::needless_range_loop)]
            for i in 0..end_points.len() {
                let index = read_be_u16_decode(self.data, &mut pos)?;
                if (index as isize) <= prev_end_point {
                    return Err(DecodeError::EndPointsOutOfOrder);
                }
                prev_end_point = index as isize;
                end_points[i] = index;
            }

            // The final point tells us our point count.
            end_points[end_points.len() - 1] as usize + 1
        };

        // Instructions are not supported.
        let instructions_length = read_be_u16_decode(self.data, &mut pos)?;
        if instructions_length > 0 {
            return Err(DecodeError::InstructionsNotSupported);
        }

        // Allocate our points right away even though the next entries
        // are flags. We want to do this so that if the allocator is
        // a bump allocator, the flags free will actually free it.
        // PORT NOTE (433b16bcb): the points slice is the dominant decoded
        // allocation; its request size is what the reference's limited
        // allocator bounds.
        if point_count.saturating_mul(core::mem::size_of::<Point>()) > max_allocation_bytes {
            return Err(DecodeError::OutOfMemory);
        }
        let mut points: Vec<Point> = vec![
            Point {
                x: 0,
                y: 0,
                on_curve: false,
            };
            point_count
        ];

        // This is EXTREMELY annoying but all the flags are separate
        // from the points so we have to do some allocation here since
        // its a dynamic amount and we need to save the values for later.
        //
        // Typical glyphs have small point counts, so use stack storage
        // first while still falling back to the caller's allocator for
        // unusually large outlines.
        // PORT NOTE: the reference uses `std.heap.stackFallback(4096, alloc)`
        // purely as an allocation-strategy optimization for the temporary flags
        // buffer; it is semantically a `[]SimpleFlags` of length `point_count`.
        // Rust has no stack-fallback allocator, so the temporary is a plain
        // `Vec<SimpleFlags>` (freed at end of scope, like the reference `defer`).
        // PORT NOTE (433b16bcb): the per-allocation limit only sees this
        // request when it spills past the 4096-byte stack half of the
        // reference's `stackFallback`, so the check mirrors that threshold.
        {
            let flags_bytes = point_count.saturating_mul(core::mem::size_of::<SimpleFlags>());
            if flags_bytes > 4096 && flags_bytes > max_allocation_bytes {
                return Err(DecodeError::OutOfMemory);
            }
        }
        let mut flags: Vec<SimpleFlags> = vec![SimpleFlags(0); point_count];
        {
            let mut point_i: usize = 0;
            while point_i < point_count {
                let flag = SimpleFlags(read_byte_decode(self.data, &mut pos)?);
                flags[point_i] = flag;
                point_i += 1;

                if flag.repeat() {
                    let repeat_count: usize = read_byte_decode(self.data, &mut pos)? as usize;
                    if point_i + repeat_count > point_count {
                        return Err(DecodeError::TooManyPoints);
                    }

                    for _ in 0..repeat_count {
                        flags[point_i] = flag;
                        point_i += 1;
                    }
                }
            }
        }

        // Go through x coordinate deltas
        let mut x: i32 = 0;
        for (flag, point) in flags.iter().zip(points.iter_mut()) {
            let dx: i32 = if flag.x_short() {
                if flag.x_repeat_or_sign() {
                    read_byte_decode(self.data, &mut pos)? as i32
                } else {
                    -(read_byte_decode(self.data, &mut pos)? as i32)
                }
            } else if !flag.x_repeat_or_sign() {
                read_be_i16_decode(self.data, &mut pos)? as i32
            } else {
                0
            };

            x = x.checked_add(dx).ok_or(DecodeError::CoordinateOverflow)?;
            point.x = x;
        }

        // Go through y coordinate deltas
        let mut y: i32 = 0;
        for (flag, point) in flags.iter().zip(points.iter_mut()) {
            let dy: i32 = if flag.y_short() {
                if flag.y_repeat_or_sign() {
                    read_byte_decode(self.data, &mut pos)? as i32
                } else {
                    -(read_byte_decode(self.data, &mut pos)? as i32)
                }
            } else if !flag.y_repeat_or_sign() {
                read_be_i16_decode(self.data, &mut pos)? as i32
            } else {
                0
            };

            y = y.checked_add(dy).ok_or(DecodeError::CoordinateOverflow)?;
            point.y = y;
            point.on_curve = flag.on_curve();
        }

        Ok(Outline {
            points,
            contours: end_points,
        })
    }
}

impl<'a> Glyf<'a> {
    /// Initialize the table from the provided data.
    ///
    /// This DOES NOT COPY the data, it only stores a pointer to it.
    ///
    /// The lifetime of this struct, then, is the same as the
    /// lifetime of the data that is used to initialize it.
    pub fn init(data: &'a [u8]) -> Glyf<'a> {
        Glyf { data }
    }

    /// Retrieve the entry at the provided offset.
    pub fn entry(&self, index: usize) -> Result<Entry<'a>, EntryInitError> {
        Entry::init(&self.data[index..])
    }
}

// --- big-endian / cursor read helpers ---
//
// PORT NOTE: these stand in for the reference's `fixedBufferStream`+`reader`
// reads (`readInt(..., .big)`, `readByte`, `skipBytes`). Each returns the
// matching error variant on underflow so the three error families
// (`EntryInitError`/`SizeError`/`DecodeError`) line up with the reference's
// `error{EndOfStream}`-bearing error sets at each call site.

fn read_be_i16_init(data: &[u8], pos: &mut usize) -> Result<i16, EntryInitError> {
    let bytes: [u8; 2] = data
        .get(*pos..*pos + 2)
        .ok_or(EntryInitError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 2;
    Ok(i16::from_be_bytes(bytes))
}

fn read_be_u16_size(data: &[u8], pos: &mut usize) -> Result<u16, SizeError> {
    let bytes: [u8; 2] = data
        .get(*pos..*pos + 2)
        .ok_or(SizeError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 2;
    Ok(u16::from_be_bytes(bytes))
}

fn read_byte_size(data: &[u8], pos: &mut usize) -> Result<u8, SizeError> {
    let b = *data.get(*pos).ok_or(SizeError::EndOfStream)?;
    *pos += 1;
    Ok(b)
}

fn skip_bytes_size(data: &[u8], pos: &mut usize, n: usize) -> Result<(), SizeError> {
    // The reference `skipBytes` errors with EndOfStream if fewer than `n`
    // bytes remain.
    if *pos + n > data.len() {
        return Err(SizeError::EndOfStream);
    }
    *pos += n;
    Ok(())
}

fn read_be_u16_decode(data: &[u8], pos: &mut usize) -> Result<u16, DecodeError> {
    let bytes: [u8; 2] = data
        .get(*pos..*pos + 2)
        .ok_or(DecodeError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 2;
    Ok(u16::from_be_bytes(bytes))
}

fn read_be_i16_decode(data: &[u8], pos: &mut usize) -> Result<i16, DecodeError> {
    let bytes: [u8; 2] = data
        .get(*pos..*pos + 2)
        .ok_or(DecodeError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 2;
    Ok(i16::from_be_bytes(bytes))
}

fn read_byte_decode(data: &[u8], pos: &mut usize) -> Result<u8, DecodeError> {
    let b = *data.get(*pos).ok_or(DecodeError::EndOfStream)?;
    *pos += 1;
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedded;
    use crate::opentype::head;
    use crate::options;

    /// TESTING ONLY
    ///
    /// Retrieves the glyf at the provided index from the provided font.
    ///
    /// Returns it in a tuple with the expected length based on the loca table, and the entry.
    // PORT NOTE: the reference `getGlyph` is a `pub fn` guarded by
    //   comptime if (!@import("builtin").is_test) @compileError("...testing only!");
    // i.e. it only exists in test builds. We place it in the `#[cfg(test)]`
    // module, which is the Rust equivalent of that guard.
    fn get_glyph<'a>(
        font: &sfnt::Sfnt<'a>,
        index: usize,
    ) -> Result<(usize, Entry<'a>), Box<dyn core::fmt::Debug>> {
        let glyf = Glyf::init(font.get_table(b"glyf").unwrap());
        let head = head::Head::init(font.get_table(b"head").unwrap())
            .map_err(|e| Box::new(e) as Box<dyn core::fmt::Debug>)?;
        let loca = font.get_table(b"loca").unwrap();

        let start_offset: usize = match head.indexToLocFormat {
            0 => loca_u16(loca, index) as usize * 2,
            1 => loca_u32(loca, index) as usize,
            _ => unreachable!(),
        };

        let end_offset: usize = match head.indexToLocFormat {
            0 => loca_u16(loca, index + 1) as usize * 2,
            1 => loca_u32(loca, index + 1) as usize,
            _ => unreachable!(),
        };

        let entry = glyf
            .entry(start_offset)
            .map_err(|e| Box::new(e) as Box<dyn core::fmt::Debug>)?;
        Ok((end_offset - start_offset, entry))
    }

    /// Read the big-endian `u16` at element `index` of the `loca` table bytes.
    // PORT NOTE: `std.mem.bigToNative(u16, std.mem.bytesAsSlice(u16, loca)[index])`.
    fn loca_u16(loca: &[u8], index: usize) -> u16 {
        let off = index * 2;
        u16::from_be_bytes([loca[off], loca[off + 1]])
    }

    /// Read the big-endian `u32` at element `index` of the `loca` table bytes.
    // PORT NOTE: `std.mem.bigToNative(u32, std.mem.bytesAsSlice(u32, loca)[index])`.
    fn loca_u32(loca: &[u8], index: usize) -> u32 {
        let off = index * 4;
        u32::from_be_bytes([loca[off], loca[off + 1], loca[off + 2], loca[off + 3]])
    }

    /// Append `value` as big-endian bytes of width `T` to `buf`.
    // PORT NOTE: ports the generic
    //   fn testAppendInt(buf, alloc, comptime T, value) !void
    // for the integer widths the tests use (`u16`, `i16`). Implemented as small
    // typed helpers since Rust has no comptime type parameter here.
    fn test_append_u16(buf: &mut Vec<u8>, value: u16) {
        buf.extend_from_slice(&value.to_be_bytes());
    }

    fn test_append_i16(buf: &mut Vec<u8>, value: i16) {
        buf.extend_from_slice(&value.to_be_bytes());
    }

    /// Append a glyph [`Header`] with the given fields, big-endian.
    // PORT NOTE: ports `testAppendHeader(buf, alloc, num_contours, x_min, ...)`.
    fn test_append_header(
        buf: &mut Vec<u8>,
        number_of_contours: i16,
        x_min: i16,
        y_min: i16,
        x_max: i16,
        y_max: i16,
    ) {
        test_append_i16(buf, number_of_contours);
        test_append_i16(buf, x_min);
        test_append_i16(buf, y_min);
        test_append_i16(buf, x_max);
        test_append_i16(buf, y_max);
    }

    // test "glyf"
    #[test]
    fn glyf() {
        // lib-vt source archives intentionally exclude full Ghostty font fixtures.
        if options::ARTIFACT == options::Artifact::Lib {
            return;
        }

        // Cozette because it doesn't have any hinting.
        let test_font = embedded::COZETTE;

        let font = sfnt::Sfnt::init(test_font).unwrap();

        // Cozette doesn't actually include a glyph for notdef,
        // but does include a glyph for `\0` (nul), at index 1.
        let (len_nul, glyph_nul) = get_glyph(&font, 1).unwrap();
        assert!(glyph_nul.entry_type() == Type::Simple);
        // It is legal for there to be extra data between two entries, just
        // as long as the next entry starts after the previous one ends, so
        // it's okay for the parsed size of the entry to be less than the size
        // determined from the difference between subsequent loca offsets.
        assert!(len_nul >= glyph_nul.size().unwrap());

        // Glyph "A" is at index 66.
        let (len_a, glyph_a) = get_glyph(&font, 66).unwrap();
        assert!(glyph_a.entry_type() == Type::Simple);
        assert!(len_a >= glyph_a.size().unwrap());

        let outline_a = glyph_a.decode().unwrap();
        assert_eq!(
            glyph_a.header.numberOfContours as usize,
            outline_a.contours.len()
        );
        assert!(!outline_a.points.is_empty());

        // Glyph "Ĩ" is at index 265.
        let (len_itilde, glyph_itilde) = get_glyph(&font, 265).unwrap();
        assert!(glyph_itilde.entry_type() == Type::Simple);
        assert!(len_itilde >= glyph_itilde.size().unwrap());
    }

    // test "glyf: decode triangle"
    // PORT NOTE: the reference pushes `0x01` three times via separate
    // `try buf.append(alloc, 0x01); // on curve` statements; kept verbatim, so
    // clippy's `same_item_push` (which would collapse them) is allowed here.
    #[test]
    #[allow(clippy::same_item_push)]
    fn glyf_decode_triangle() {
        let mut buf: Vec<u8> = Vec::new();

        test_append_header(&mut buf, 1, 100, 100, 900, 900);
        test_append_u16(&mut buf, 2); // endPtsOfContours[0]
        test_append_u16(&mut buf, 0); // instructionLength
        buf.push(0x01); // on curve
        buf.push(0x01); // on curve
        buf.push(0x01); // on curve
        test_append_i16(&mut buf, 500);
        test_append_i16(&mut buf, -400);
        test_append_i16(&mut buf, 800);
        test_append_i16(&mut buf, 900);
        test_append_i16(&mut buf, -800);
        test_append_i16(&mut buf, 0);

        let glyph = Entry::init(&buf).unwrap();
        let outline = glyph.decode().unwrap();

        assert_eq!(100i16, glyph.header.xMin);
        assert_eq!(900i16, glyph.header.xMax);
        assert_eq!(1usize, outline.contours.len());
        assert_eq!(3usize, outline.points.len());
        let contour = outline.contour(0);
        assert_eq!(3usize, contour.len());
        assert_eq!(
            Point {
                x: 500,
                y: 900,
                on_curve: true
            },
            contour[0]
        );
        assert_eq!(
            Point {
                x: 100,
                y: 100,
                on_curve: true
            },
            contour[1]
        );
        assert_eq!(
            Point {
                x: 900,
                y: 100,
                on_curve: true
            },
            contour[2]
        );
    }

    // test "glyf: decode multiple contours"
    // PORT NOTE: the reference loops `for (0..4) |_| try buf.append(alloc, 0x01)`;
    // kept verbatim, so clippy's `same_item_push` is allowed here.
    #[test]
    #[allow(clippy::same_item_push)]
    fn glyf_decode_multiple_contours() {
        let mut buf: Vec<u8> = Vec::new();

        test_append_header(&mut buf, 2, 0, 0, 30, 10);
        test_append_u16(&mut buf, 1); // first contour ends at point 1
        test_append_u16(&mut buf, 3); // second contour ends at point 3
        test_append_u16(&mut buf, 0); // instructionLength
        for _ in 0..4 {
            buf.push(0x01); // on curve
        }
        for dx in [0i16, 10, 10, 10] {
            test_append_i16(&mut buf, dx);
        }
        for dy in [0i16, 0, 10, 0] {
            test_append_i16(&mut buf, dy);
        }

        let glyph = Entry::init(&buf).unwrap();
        let outline = glyph.decode().unwrap();

        assert_eq!(2usize, outline.contours.len());
        assert_eq!(4usize, outline.points.len());
        assert_eq!(1u16, outline.contours[0]);
        assert_eq!(3u16, outline.contours[1]);
        assert_eq!(2usize, outline.contour(0).len());
        assert_eq!(2usize, outline.contour(1).len());
        assert_eq!(outline.points[0..2].as_ptr(), outline.contour(0).as_ptr());
        assert_eq!(outline.points[2..4].as_ptr(), outline.contour(1).as_ptr());
    }

    // test "glyf: decode repeat and short vector flags"
    #[test]
    fn glyf_decode_repeat_and_short_vector_flags() {
        let mut buf: Vec<u8> = Vec::new();

        test_append_header(&mut buf, 1, 0, -16, 16, 0);
        test_append_u16(&mut buf, 3); // four points
        test_append_u16(&mut buf, 0); // instructionLength
        buf.push(0x01 | 0x02 | 0x04 | 0x08 | 0x10); // on, x short positive, y short negative, repeat
        buf.push(3); // repeat for the next three points
        for dx in [1u8, 2, 4, 8] {
            buf.push(dx);
        }
        for dy in [1u8, 2, 4, 8] {
            buf.push(dy);
        }

        let glyph = Entry::init(&buf).unwrap();
        let outline = glyph.decode().unwrap();

        assert_eq!(4usize, outline.points.len());
        assert_eq!(
            Point {
                x: 1,
                y: -1,
                on_curve: true
            },
            outline.points[0]
        );
        assert_eq!(
            Point {
                x: 3,
                y: -3,
                on_curve: true
            },
            outline.points[1]
        );
        assert_eq!(
            Point {
                x: 7,
                y: -7,
                on_curve: true
            },
            outline.points[2]
        );
        assert_eq!(
            Point {
                x: 15,
                y: -15,
                on_curve: true
            },
            outline.points[3]
        );
    }

    // test "glyf: decode off curve and same coordinate flags"
    #[test]
    fn glyf_decode_off_curve_and_same_coordinate_flags() {
        let mut buf: Vec<u8> = Vec::new();

        test_append_header(&mut buf, 1, 0, 0, 7, 9);
        test_append_u16(&mut buf, 1); // two points
        test_append_u16(&mut buf, 0); // instructionLength
        buf.push(0x10 | 0x20); // off curve, x same, y same
        buf.push(0x01 | 0x02 | 0x04 | 0x10 | 0x20); // on curve, short positive x/y
        buf.push(7); // x delta
        buf.push(9); // y delta

        let glyph = Entry::init(&buf).unwrap();
        let outline = glyph.decode().unwrap();

        assert_eq!(
            Point {
                x: 0,
                y: 0,
                on_curve: false
            },
            outline.points[0]
        );
        assert_eq!(
            Point {
                x: 7,
                y: 9,
                on_curve: true
            },
            outline.points[1]
        );
    }

    // test "glyf: decode one-point contour"
    #[test]
    fn glyf_decode_one_point_contour() {
        let mut buf: Vec<u8> = Vec::new();

        test_append_header(&mut buf, 1, 0, 0, 0, 0);
        test_append_u16(&mut buf, 0); // endPtsOfContours[0]
        test_append_u16(&mut buf, 0); // instructionLength
        buf.push(0x01 | 0x10 | 0x20); // on curve, x same, y same

        let glyph = Entry::init(&buf).unwrap();
        let outline = glyph.decode().unwrap();

        assert_eq!(1usize, outline.points.len());
        assert_eq!(1usize, outline.contours.len());
        assert_eq!(0u16, outline.contours[0]);
        assert_eq!(
            Point {
                x: 0,
                y: 0,
                on_curve: true
            },
            outline.contour(0)[0]
        );
    }

    // test "glyf: decode contour ending at max point index"
    #[test]
    fn glyf_decode_contour_ending_at_max_point_index() {
        let mut buf: Vec<u8> = Vec::new();

        test_append_header(&mut buf, 1, 0, 0, 0, 0);
        test_append_u16(&mut buf, u16::MAX); // 65536 points
        test_append_u16(&mut buf, 0); // instructionLength

        let flag: u8 = 0x01 | 0x10 | 0x20; // on curve, x same, y same
        let mut remaining: usize = u16::MAX as usize + 1;
        while remaining > 0 {
            let run = remaining.min(256);
            if run == 1 {
                buf.push(flag);
            } else {
                buf.push(flag | 0x08); // repeat
                buf.push((run - 1) as u8);
            }
            remaining -= run;
        }

        let glyph = Entry::init(&buf).unwrap();
        let outline = glyph.decode().unwrap();

        assert_eq!(65536usize, outline.points.len());
        assert_eq!(65536usize, outline.contour(0).len());
    }

    // test "glyf: reject glyphs with instructions and composite glyphs"
    #[test]
    fn glyf_reject_glyphs_with_instructions_and_composite_glyphs() {
        // lib-vt source archives intentionally exclude full Ghostty font fixtures.
        if options::ARTIFACT == options::Artifact::Lib {
            return;
        }

        let test_font = embedded::JETBRAINS_MONO;

        let font = sfnt::Sfnt::init(test_font).unwrap();

        let (len_notdef, glyph_notdef) = get_glyph(&font, 0).unwrap();
        assert_eq!(100, len_notdef);
        assert!(glyph_notdef.entry_type() == Type::Simple);
        assert_eq!(
            Err(SizeError::InstructionsNotSupported),
            glyph_notdef.size(),
        );
        assert_eq!(
            Err(DecodeError::InstructionsNotSupported),
            glyph_notdef.decode().map(|_| ()),
        );

        // Glyph "Á" is at index 2.
        let (len_aacute, glyph_aacute) = get_glyph(&font, 2).unwrap();
        assert_eq!(24, len_aacute);
        assert!(glyph_aacute.entry_type() == Type::Composite);
        assert_eq!(Err(SizeError::CompositeNotSupported), glyph_aacute.size(),);
        assert_eq!(
            Err(DecodeError::CompositeNotSupported),
            glyph_aacute.decode().map(|_| ()),
        );
    }

    // test "glyf: reject truncated"
    #[test]
    fn glyf_reject_truncated() {
        // lib-vt source archives intentionally exclude full Ghostty font fixtures.
        if options::ARTIFACT == options::Artifact::Lib {
            return;
        }

        // Cozette because it doesn't have any hinting.
        let test_font = embedded::COZETTE;

        let font = sfnt::Sfnt::init(test_font).unwrap();

        let (_, mut glyph_nul) = get_glyph(&font, 1).unwrap();
        assert!(glyph_nul.entry_type() == Type::Simple);
        // Mess with the entry's data slice, truncating
        // it before the full length (which is 228 bytes).
        glyph_nul.data = &glyph_nul.data[0..227 - HEADER_SIZE];
        assert_eq!(Err(SizeError::EndOfStream), glyph_nul.size());
        assert_eq!(
            Err(DecodeError::EndOfStream),
            glyph_nul.decode().map(|_| ())
        );
    }

    // test "glyf: reject endpoints out of order"
    #[test]
    fn glyf_reject_endpoints_out_of_order() {
        // lib-vt source archives intentionally exclude full Ghostty font fixtures.
        if options::ARTIFACT == options::Artifact::Lib {
            return;
        }

        // Cozette because it doesn't have any hinting.
        //
        // Also we copy it with the allocator so we can mess with it.
        // PORT NOTE: `alloc.dupe(u8, cozette[0..])`; in Rust an owned `Vec<u8>`.
        let test_font: Vec<u8> = embedded::COZETTE.to_vec();

        let font = sfnt::Sfnt::init(&test_font).unwrap();

        let (_, glyph_nul) = get_glyph(&font, 1).unwrap();
        assert!(glyph_nul.entry_type() == Type::Simple);
        // Mess with the entry's data, insert a 0 in the middle of the endpoints.
        //
        // Because we know the underlying data is something we
        // copied, we can just const cast it back to mutable lol.
        // PORT NOTE: the reference `@constCast`es the entry's data slice and
        // writes element [3] of its `[]u16` view to 0. We can't write through a
        // shared `&[u8]`, so we mutate the owned font copy at the equivalent byte
        // offset (the entry data starts after the 10-byte header at the glyph's
        // loca offset) and re-parse the entry from the mutated buffer. Element 3
        // of the entry-data `u16` view is bytes [6..8] of the entry data, i.e.
        // bytes [glyph_start + HEADER_SIZE + 6 ..] of the font.
        let (glyph_start, _) = {
            let glyf = Glyf::init(font.get_table(b"glyf").unwrap());
            let head = head::Head::init(font.get_table(b"head").unwrap()).unwrap();
            let loca = font.get_table(b"loca").unwrap();
            let start: usize = match head.indexToLocFormat {
                0 => loca_u16(loca, 1) as usize * 2,
                1 => loca_u32(loca, 1) as usize,
                _ => unreachable!(),
            };
            // glyf table start offset within the font, plus the entry start
            let glyf_off = {
                let mut off = 0usize;
                for record in &font.directory.records {
                    if &record.tag == b"glyf" {
                        off = record.offset as usize;
                        break;
                    }
                }
                off
            };
            let _ = glyf;
            (glyf_off + start, ())
        };
        let mut mutated = test_font.clone();
        let elem3 = glyph_start + HEADER_SIZE + 3 * 2;
        mutated[elem3] = 0;
        mutated[elem3 + 1] = 0;

        let font2 = sfnt::Sfnt::init(&mutated).unwrap();
        let (_, glyph_nul) = get_glyph(&font2, 1).unwrap();
        assert_eq!(Err(SizeError::EndPointsOutOfOrder), glyph_nul.size());
        assert_eq!(
            Err(DecodeError::EndPointsOutOfOrder),
            glyph_nul.decode().map(|_| ()),
        );
    }

    // test "glyf: reject too many points"
    #[test]
    fn glyf_reject_too_many_points() {
        // lib-vt source archives intentionally exclude full Ghostty font fixtures.
        if options::ARTIFACT == options::Artifact::Lib {
            return;
        }

        // Cozette because it doesn't have any hinting.
        //
        // Also we copy it with the allocator so we can mess with it.
        let test_font: Vec<u8> = embedded::COZETTE.to_vec();

        let font = sfnt::Sfnt::init(&test_font).unwrap();

        let (_, glyph_nul) = get_glyph(&font, 1).unwrap();
        assert!(glyph_nul.entry_type() == Type::Simple);
        // Mess with the entry's data, make the final two bytes of the flags
        // array be a large number repeat to exceed the correct points count.
        //
        // Because we know the underlying data is something we
        // copied, we can just const cast it back to mutable lol.
        // PORT NOTE: the reference indexes the entry's data slice (which starts
        // after the header) at [107] and [108]. We mutate the equivalent bytes of
        // the owned font copy (glyph entry-data start + 107/108) and re-parse.
        let glyf_off = {
            let mut off = 0usize;
            for record in &font.directory.records {
                if &record.tag == b"glyf" {
                    off = record.offset as usize;
                    break;
                }
            }
            off
        };
        let head = head::Head::init(font.get_table(b"head").unwrap()).unwrap();
        let loca = font.get_table(b"loca").unwrap();
        let start: usize = match head.indexToLocFormat {
            0 => loca_u16(loca, 1) as usize * 2,
            1 => loca_u32(loca, 1) as usize,
            _ => unreachable!(),
        };
        // entry data begins after the 10-byte header
        let entry_data_off = glyf_off + start + HEADER_SIZE;

        let mut mutated = test_font.clone();
        mutated[entry_data_off + 107] |= 0x08;
        mutated[entry_data_off + 108] = 0xFF;

        let font2 = sfnt::Sfnt::init(&mutated).unwrap();
        let (_, glyph_nul) = get_glyph(&font2, 1).unwrap();
        assert_eq!(Err(SizeError::TooManyPoints), glyph_nul.size());
        assert_eq!(
            Err(DecodeError::TooManyPoints),
            glyph_nul.decode().map(|_| ())
        );
    }

    // test "glyf: zero-contour glyph can be header-only"
    #[test]
    fn glyf_zero_contour_glyph_can_be_header_only() {
        let header = Header {
            numberOfContours: 0,
            xMin: 0,
            yMin: 0,
            xMax: 0,
            yMax: 0,
        };
        // PORT NOTE: `std.mem.asBytes(&header)` of the `extern struct` is its
        // 10-byte big-endian on-disk image. We build that image explicitly.
        let mut header_bytes: Vec<u8> = Vec::new();
        test_append_header(
            &mut header_bytes,
            header.numberOfContours,
            header.xMin,
            header.yMin,
            header.xMax,
            header.yMax,
        );
        let glyph = Entry::init(&header_bytes).unwrap();
        assert_eq!(HEADER_SIZE, glyph.size().unwrap());

        let outline = glyph.decode().unwrap();
        assert_eq!(0usize, outline.points.len());
        assert_eq!(0usize, outline.contours.len());
    }

    // test "glyf: zero-contour glyph can include instruction length"
    #[test]
    fn glyf_zero_contour_glyph_can_include_instruction_length() {
        let mut buf: Vec<u8> = Vec::new();

        test_append_header(&mut buf, 0, 0, 0, 0, 0);
        test_append_u16(&mut buf, 0); // instructionLength

        let glyph = Entry::init(&buf).unwrap();
        assert_eq!(HEADER_SIZE + 2, glyph.size().unwrap());

        let outline = glyph.decode().unwrap();
        assert_eq!(0usize, outline.points.len());
        assert_eq!(0usize, outline.contours.len());
    }

    // test "glyf: zero-contour glyph rejects instructions"
    #[test]
    fn glyf_zero_contour_glyph_rejects_instructions() {
        let mut buf: Vec<u8> = Vec::new();

        test_append_header(&mut buf, 0, 0, 0, 0, 0);
        test_append_u16(&mut buf, 1); // instructionLength

        let glyph = Entry::init(&buf).unwrap();
        assert_eq!(Err(SizeError::InstructionsNotSupported), glyph.size());
        assert_eq!(
            Err(DecodeError::InstructionsNotSupported),
            glyph.decode().map(|_| ()),
        );
    }
}
