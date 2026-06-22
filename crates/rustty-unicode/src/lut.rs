// This whole file is based on the algorithm described here:
// https://here-be-braces.com/fast-lookup-of-unicode-properties/

use std::collections::HashMap;

/// Error type for table generation.
///
/// PARITY: mirrors the Zig error set the reference `generate` can return
/// (`error.BlockTooLarge`, `error.Stage2TooLarge`).
#[derive(Debug, PartialEq, Eq)]
pub enum GenerateError {
    BlockTooLarge,
    Stage2TooLarge,
}

/// Context trait for [`Generator`].
///
/// PARITY: the reference passes a comptime `Context` type with two functions:
///   - `get(Context, u21) Elem`: returns the mapping for a given codepoint
///   - `eql(Context, Elem, Elem) bool`: returns true if two mappings are equal
pub trait Context<Elem> {
    /// returns the mapping for a given codepoint
    fn get(&self, cp: u32) -> Elem;
    /// returns true if two mappings are equal
    fn eql(&self, a: &Elem, b: &Elem) -> bool;
}

/// Creates a type that is able to generate a 3-level lookup table
/// from a Unicode codepoint to a mapping of type Elem. The lookup table
/// generally is expected to be codegen'd and then reloaded, although it
/// can in theory be generated at runtime.
///
/// Context must have two functions:
///   - `get(Context, u21) Elem`: returns the mapping for a given codepoint
///   - `eql(Context, Elem, Elem) bool`: returns true if two mappings are equal
///
pub struct Generator<Elem, Ctx: Context<Elem>> {
    pub ctx: Ctx,
    _marker: std::marker::PhantomData<Elem>,
}

const BLOCK_SIZE: usize = 256;
type Block = [u16; BLOCK_SIZE];

/// Mapping of a block to its index in the stage2 array.
///
/// PARITY: the reference `BlockMap` is a `std.HashMap` with a custom Wyhash
/// hasher and `std.mem.eql`-based equality over the block contents. Rust's
/// `HashMap<Block, u16>` derives the same structural hashing/equality over the
/// fixed-size `[u16; 256]` key.
type BlockMap = HashMap<Block, u16>;

impl<Elem: Clone, Ctx: Context<Elem>> Generator<Elem, Ctx> {
    pub fn new(ctx: Ctx) -> Self {
        Self {
            ctx,
            _marker: std::marker::PhantomData,
        }
    }

    /// Generate the lookup tables. The arrays in the return value
    /// are owned by the caller and must be freed.
    pub fn generate(&self) -> Result<Tables<Elem>, GenerateError> {
        // Maps block => stage2 index
        let mut blocks_map: BlockMap = BlockMap::new();

        // Our stages
        let mut stage1: Vec<u16> = Vec::new();
        let mut stage2: Vec<u16> = Vec::new();
        let mut stage3: Vec<Elem> = Vec::new();

        let mut block: Block = [0; BLOCK_SIZE];
        let mut block_len: u16 = 0;
        let max_cp: u32 = 0x1F_FFFF; // std.math.maxInt(u21)
        for cp in 0..=max_cp {
            // Get our block value and find the matching result value
            // in our list of possible values in stage3. This way, each
            // possible mapping only gets one entry in stage3.
            let elem = self.ctx.get(cp);
            let block_idx = {
                let mut found: Option<usize> = None;
                for (i, item) in stage3.iter().enumerate() {
                    if self.ctx.eql(item, &elem) {
                        found = Some(i);
                        break;
                    }
                }
                match found {
                    Some(i) => i,
                    None => {
                        let idx = stage3.len();
                        stage3.push(elem);
                        idx
                    }
                }
            };

            // The block stores the mapping to the stage3 index
            block[block_len as usize] =
                u16::try_from(block_idx).map_err(|_| GenerateError::BlockTooLarge)?;
            block_len += 1;

            // If we still have space and we're not done with codepoints,
            // we keep building up the block. Conversely: we finalize this
            // block if we've filled it or are out of codepoints.
            if (block_len as usize) < BLOCK_SIZE && cp != max_cp {
                continue;
            }
            if (block_len as usize) < BLOCK_SIZE {
                for slot in block.iter_mut().skip(block_len as usize) {
                    *slot = 0;
                }
            }

            // Look for the stage2 index for this block. If it doesn't exist
            // we add it to stage2 and update the mapping.
            let value = match blocks_map.get(&block).copied() {
                Some(v) => v,
                None => {
                    let v =
                        u16::try_from(stage2.len()).map_err(|_| GenerateError::Stage2TooLarge)?;
                    blocks_map.insert(block, v);
                    for &entry in &block[0..block_len as usize] {
                        stage2.push(entry);
                    }
                    v
                }
            };

            // Map stage1 => stage2 and reset our block
            stage1.push(value);
            block_len = 0;
        }

        // All of our lengths must fit in a u16 for this to work
        assert!(stage1.len() <= u16::MAX as usize);
        assert!(stage2.len() <= u16::MAX as usize);
        assert!(stage3.len() <= u16::MAX as usize);

        Ok(Tables {
            stage1,
            stage2,
            stage3,
        })
    }
}

/// Creates a type that given a 3-level lookup table, can be used to
/// look up a mapping for a given codepoint, encode it out to Zig, etc.
///
/// PARITY: the reference `Tables` borrows `[]const` slices into the
/// codegen'd arrays; rustty's runtime tables (`props_table`, `symbols_table`)
/// borrow `&'static [..]` into the embedded generated data, so this carries a
/// lifetime over the backing slices.
pub struct Tables<Elem> {
    pub stage1: Vec<u16>,
    pub stage2: Vec<u16>,
    pub stage3: Vec<Elem>,
}

/// How a stage3 element renders itself when written out as Zig source.
///
/// PARITY: the reference `writeZig` switches at comptime on whether `Elem`
/// has a `format` decl (structs like `Properties`) vs. a primitive (`bool`),
/// printing `"{f},"` or `"{},"` respectively. This trait captures that choice.
pub trait WriteZig {
    fn write_zig_elem(&self) -> String;
}

impl WriteZig for bool {
    fn write_zig_elem(&self) -> String {
        format!("{self}")
    }
}

impl<Elem: WriteZig> Tables<Elem> {
    /// Writes the lookup table as Zig to the given writer. The
    /// written file exports three constants: stage1, stage2, and
    /// stage3. These can be used to rebuild the lookup table in Zig.
    pub fn write_zig(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "//! This file is auto-generated. Do not edit.\n\npub fn Tables(comptime Elem: type) type {{\n    return struct {{\npub const stage1: [{}]u16 = .{{",
            self.stage1.len()
        ));
        for entry in &self.stage1 {
            out.push_str(&format!("{entry},"));
        }

        out.push_str(&format!(
            "\n}};\n\npub const stage2: [{}]u16 = .{{",
            self.stage2.len()
        ));
        for entry in &self.stage2 {
            out.push_str(&format!("{entry},"));
        }
        out.push_str("};");

        out.push_str(&format!(
            "\n\npub const stage3: [{}]Elem = .{{",
            self.stage3.len()
        ));
        for entry in &self.stage3 {
            out.push_str(&entry.write_zig_elem());
            out.push(',');
        }
        out.push_str("};\n    };\n}\n");
        out
    }
}

/// Borrowed view over a 3-level lookup table backed by `'static` slices.
///
/// PARITY: this is the shape `props_table.table` / `symbols_table.table` use —
/// the reference's `Tables(Elem)` with `[]const` fields pointing at the
/// generated `stage1`/`stage2`/`stage3` constants.
#[derive(Clone, Copy)]
pub struct TablesRef<'a, Elem: 'a> {
    pub stage1: &'a [u16],
    pub stage2: &'a [u16],
    pub stage3: &'a [Elem],
}

impl<'a, Elem: Copy> TablesRef<'a, Elem> {
    /// Given a codepoint, returns the mapping for that codepoint.
    #[inline]
    pub fn get(&self, cp: u32) -> Elem {
        let high = cp >> 8;
        let low = cp & 0xFF;
        self.stage3[self.stage2[self.stage1[high as usize] as usize + low as usize] as usize]
    }
}
