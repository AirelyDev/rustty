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

        // One newline, not two: the reference's literal here is `\\` (empty)
        // then `\\pub const stage3...`, unlike the stage2 header above
        // (parity S215-F1; CORRECTION: this pushed "\n\n").
        out.push_str(&format!(
            "\npub const stage3: [{}]Elem = .{{",
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
// PERF(port): the reference's `[]const` slices index without any runtime
// bounds checks in ReleaseFast; the port's `&[u16]` slices made every stage
// length opaque, so each of the three stage lookups paid a bounds-check
// branch. Const-generic fixed-size array references restore the
// comptime-known lengths of the generated constants (props: [u16; 8192] /
// [u16; 31744] / [Properties; 30]; symbols: [u16; 8192] / [u16; 1792] /
// [bool; 2]) so LLVM can elide the checks.
#[derive(Clone, Copy)]
pub struct TablesRef<'a, Elem: 'a, const S1: usize, const S2: usize, const S3: usize> {
    pub stage1: &'a [u16; S1],
    pub stage2: &'a [u16; S2],
    pub stage3: &'a [Elem; S3],
}

impl<'a, Elem: Copy, const S1: usize, const S2: usize, const S3: usize>
    TablesRef<'a, Elem, S1, S2, S3>
{
    /// Given a codepoint, returns the mapping for that codepoint.
    #[inline]
    pub fn get(&self, cp: u32) -> Elem {
        // PERF(port): the reference takes `cp: u21` (lut.zig `get`), so the
        // codepoint is provably <= 0x1F_FFFF by its parameter type; the port
        // widened the parameter to u32, which made the stage1 range
        // unprovable. Mask back to the u21 domain — a no-op for every value
        // expressible in the reference — so the stage1 bounds check folds
        // away at monomorphization (S1 == 8192 == 0x20_0000 >> 8 covers the
        // full u21 >> 8 range for every generated table).
        let cp = cp & 0x1F_FFFF;
        let high = (cp >> 8) as usize;
        let low = (cp & 0xFF) as usize;
        let s2_idx = self.stage1[high] as usize + low;
        // PERF(port): the reference indexes stage2/stage3 with no runtime
        // checks in ReleaseFast. LLVM cannot range-analyze the constant
        // table contents, so the fixed sizes alone leave two residual
        // bounds-check branches here; elide them under the generator
        // invariant (every stage1 entry + 255 < stage2.len(); every stage2
        // entry < stage3.len()), which the `generated_tables_uphold_lut_
        // invariant` unit test machine-checks over the embedded constants.
        debug_assert!(s2_idx < S2);
        // SAFETY: `s2_idx = stage1[high] + low` with `low <= 0xFF`; by the
        // generator invariant (machine-checked over the generated constants
        // by the unit test below) every stage1 entry + 255 < S2.
        let s3_idx = unsafe { *self.stage2.get_unchecked(s2_idx) } as usize;
        debug_assert!(s3_idx < S3);
        // SAFETY: by the generator invariant (machine-checked by the unit
        // test below) every stage2 entry < S3.
        unsafe { *self.stage3.get_unchecked(s3_idx) }
    }
}

#[cfg(test)]
mod tests {
    use crate::generated::{props_data, symbols_data};

    /// Parity S215-F1: `write_zig` reproduces `Tables.writeZig` byte for byte.
    /// The expected text was MEASURED: `zig test` (0.16.0) on a scratch root
    /// importing the pinned `src/unicode/lut.zig`, the same three `Tables(bool)`
    /// stages, `writeZig` into a fixed writer.
    #[test]
    fn write_zig_matches_the_reference_output() {
        let t = super::Tables::<bool> {
            stage1: vec![0, 256],
            stage2: vec![0, 1, 1],
            stage3: vec![false, true],
        };
        assert_eq!(
            "//! This file is auto-generated. Do not edit.\n\npub fn Tables(comptime Elem: type) type {\n    return struct {\npub const stage1: [2]u16 = .{0,256,\n};\n\npub const stage2: [3]u16 = .{0,1,1,};\npub const stage3: [2]Elem = .{false,true,};\n    };\n}\n",
            t.write_zig(),
        );
    }

    /// Machine-checks the generator invariant `TablesRef::get` relies on to
    /// elide the stage2/stage3 bounds checks, over every embedded generated
    /// table: each stage1 entry, plus the maximum low byte (0xFF), indexes
    /// within stage2; each stage2 entry indexes within stage3.
    #[test]
    fn generated_tables_uphold_lut_invariant() {
        fn check<Elem>(stage1: &[u16], stage2: &[u16], stage3: &[Elem]) {
            for &s1 in stage1 {
                assert!(s1 as usize + 0xFF < stage2.len());
            }
            for &s2 in stage2 {
                assert!((s2 as usize) < stage3.len());
            }
        }
        check(
            &props_data::STAGE1,
            &props_data::STAGE2,
            &props_data::STAGE3,
        );
        check(
            &symbols_data::STAGE1,
            &symbols_data::STAGE2,
            &symbols_data::STAGE3,
        );
    }

    /// The getter must return exactly what the reference's plain 3-stage
    /// slice indexing returns for every codepoint in the u21 domain
    /// (including 0x10FFFF and the mask boundary 0x1F_FFFF), and masking
    /// must make out-of-u21-domain inputs (inexpressible in the reference's
    /// `cp: u21` parameter) alias their masked counterparts.
    #[test]
    fn get_matches_reference_indexing_across_u21_sweep() {
        fn reference_get<Elem: Copy>(
            stage1: &[u16],
            stage2: &[u16],
            stage3: &[Elem],
            cp: u32,
        ) -> Elem {
            let high = cp >> 8;
            let low = cp & 0xFF;
            stage3[stage2[stage1[high as usize] as usize + low as usize] as usize]
        }

        let props = crate::props_table::table();
        let symbols = crate::symbols_table::table();
        for cp in 0..=0x1F_FFFFu32 {
            assert_eq!(
                props.get(cp),
                reference_get(
                    &props_data::STAGE1,
                    &props_data::STAGE2,
                    &props_data::STAGE3,
                    cp
                ),
                "props mismatch at cp={cp:#x}"
            );
            assert_eq!(
                symbols.get(cp),
                reference_get(
                    &symbols_data::STAGE1,
                    &symbols_data::STAGE2,
                    &symbols_data::STAGE3,
                    cp
                ),
                "symbols mismatch at cp={cp:#x}"
            );
        }

        // Mask boundary: values above the u21 domain (inexpressible as the
        // reference's `u21`) must alias their masked counterparts.
        for cp in [0u32, 0x41, 0x10FFFF, 0x1F_FFFF] {
            let above = cp | 0x20_0000;
            assert_eq!(props.get(above), props.get(cp));
            assert_eq!(symbols.get(above), symbols.get(cp));
            assert_eq!(props.get(cp | 0xFFE0_0000), props.get(cp));
        }
    }
}
