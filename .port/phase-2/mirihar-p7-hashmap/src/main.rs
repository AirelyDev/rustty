//! P7-W1 T2 — the hash map's cached key/value pointers (`6112935a2`) under
//! Stacked and Tree Borrows.
//!
//! Since `6112935a2` a `HashMapUnmanaged` value caches `metadata`, `keys` and
//! `values` (all three derived once, by `OffsetHashMap::map(base)`), where it
//! used to cache only `metadata` and re-derive keys/values from the in-buffer
//! header on every access. The map lives inside the page's backing memory, and
//! every page method keeps its map value alive across other page-memory
//! accesses (the hyperlink set release, the grapheme alloc, `getOffset`). So
//! the derivation of `base` decides soundness:
//!
//! - `mutref`: `base` comes from a `&mut Page` -> `&mut [u8]` accessor
//!   (`Page::memory_mut`), evaluated at every place the reference reads
//!   `self.memory`. Each evaluation is a fresh Unique retag of the whole page
//!   memory, which invalidates the pointers the live map cached from the
//!   previous one.
//! - `raw`: `base` is the page's raw `memory_ptr` (what every
//!   `crates/rustty-vt/src/page_impl.rs` site does: `let base =
//!   self.memory_ptr();`), and `Offset::ptr` keeps its provenance. Every
//!   pointer is a SharedRW sibling of the one raw base, so interleaving is
//!   fine.
//!
//! The hash map is the REAL `crates/rustty-vt/src/hash_map.rs` (`#[path]`),
//! post-`6112935a2`. The page functions below mirror
//! `crates/rustty-vt/src/page_impl.rs` statement for statement, with each
//! reference `self.memory` spelled `mem!(self)` (the Rust port hoists it into
//! one `let base = self.memory_ptr();`, which in `raw` mode is the same pointer
//! with the same tag, so the hoist changes nothing).
//!
//! Usage: `mirihar-p7-hashmap <mode>_<scenario>`; see `run-all.sh`.

#![allow(dead_code)]

#[path = "../../../../crates/rustty-vt/src/hash_map.rs"]
mod hash_map;

use hash_map::{AutoContext, AutoOffsetHashMap, DEFAULT_MAX_LOAD_PERCENTAGE};
use rustty_render_types::term_size::{get_offset, Offset, OffsetBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// `base` = the page's raw `memory_ptr` (the shipped port).
    Raw,
    /// `base` = `self.memory_mut().as_mut_ptr()`, a `&mut Page`-derived
    /// `&mut [u8]` projection (the hazard the LIFETIMES rows forbid).
    MutRef,
}

/// A page cell. Only its address matters here (`getOffset(Cell, ...)`); the
/// flags stand in for `cell.hyperlink` / `cell.content_tag`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Cell {
    codepoint: u32,
    hyperlink: bool,
    grapheme: bool,
}

/// `hyperlink.Id`.
type Id = u16;

/// `OffsetSlice` (the grapheme map value): `{ offset: Offset(u21), len }`.
#[derive(Clone, Copy)]
struct OffsetSlice {
    offset: Offset<u32>,
    len: usize,
}

type GraphemeMap = AutoOffsetHashMap<Offset<Cell>, OffsetSlice, DEFAULT_MAX_LOAD_PERCENTAGE>;
type HyperlinkMap = AutoOffsetHashMap<Offset<Cell>, Id, DEFAULT_MAX_LOAD_PERCENTAGE>;

const COLS: usize = 16;
const CHUNK: usize = 4; // grapheme_chunk_len
const CHUNKS: usize = 16;
const SET_CAP: usize = 8;

/// The grapheme `BitmapAllocator` shape: one bitmap word and the chunk array,
/// both inside the page memory, reached through `base`.
struct GraphemeAlloc {
    bitmap: Offset<u64>,
    chunks: Offset<u32>,
}

impl GraphemeAlloc {
    /// `alloc(u21, base, n)`: marks a chunk used (0 = free, `ffe015ee5`) and
    /// returns it as a slice of the page memory.
    unsafe fn alloc<'a>(&mut self, base: *mut u8, n: usize) -> &'a mut [u32] {
        assert!(n <= CHUNK);
        let bm = unsafe { self.bitmap.ptr(base) };
        let idx = unsafe { (*bm).count_ones() } as usize;
        assert!(idx < CHUNKS);
        unsafe { *bm |= 1u64 << idx };
        let chunk = unsafe { self.chunks.ptr(base).add(idx * CHUNK) };
        unsafe { core::slice::from_raw_parts_mut(chunk, n) }
    }
}

/// The hyperlink `RefCountedSet` shape: a ref-count array inside the page
/// memory, reached through `base`.
struct HyperlinkSet {
    refs: Offset<u32>,
}

impl HyperlinkSet {
    /// `release(base, id)`: a write to the set's item in the page memory.
    unsafe fn release(&mut self, base: *mut u8, id: Id) {
        let refs = unsafe { self.refs.ptr(base) };
        unsafe { *refs.add(id as usize) -= 1 };
    }

    unsafe fn use_id(&mut self, base: *mut u8, id: Id) {
        let refs = unsafe { self.refs.ptr(base) };
        unsafe { *refs.add(id as usize) += 1 };
    }
}

struct Page {
    memory_ptr: *const u8,
    memory_len: usize,
    mode: Mode,
    cells: Offset<Cell>,
    grapheme_alloc: GraphemeAlloc,
    grapheme_map: GraphemeMap,
    hyperlink_map: HyperlinkMap,
    hyperlink_set: HyperlinkSet,
}

/// The reference's `self.memory`, evaluated at each use.
macro_rules! mem {
    ($self:ident) => {
        $self.mem()
    };
}

fn align(v: usize, a: usize) -> usize {
    v.next_multiple_of(a)
}

impl Page {
    /// Zero-filled backing memory (fresh OS pages; `allocate_zeroed` under
    /// Miri), maps set up with `initAssumeZeroed` as `d2ff6d77a`'s page init
    /// will.
    fn init(mode: Mode) -> Page {
        let cells_start = 0;
        let bitmap_start = align(cells_start + COLS * core::mem::size_of::<Cell>(), 8);
        let chunks_start = bitmap_start + 8;
        let refs_start = align(chunks_start + CHUNKS * CHUNK * 4, 4);
        let gm_layout = GraphemeMap::layout(8);
        let gm_start = align(refs_start + SET_CAP * 4, GraphemeMap::base_align());
        let hm_layout = HyperlinkMap::layout(8);
        let hm_start = align(gm_start + gm_layout.total_size, HyperlinkMap::base_align());
        let total = hm_start + hm_layout.total_size;

        let layout = std::alloc::Layout::from_size_align(total, 64).unwrap();
        let memory = unsafe { std::alloc::alloc_zeroed(layout) };
        assert!(!memory.is_null());
        let buf = OffsetBuf::init(memory as *const u8);

        Page {
            memory_ptr: memory,
            memory_len: total,
            mode,
            cells: buf.member(cells_start),
            grapheme_alloc: GraphemeAlloc {
                bitmap: buf.member(bitmap_start),
                chunks: buf.member(chunks_start),
            },
            grapheme_map: unsafe { GraphemeMap::init_assume_zeroed(buf.add(gm_start), gm_layout) },
            hyperlink_map: unsafe {
                HyperlinkMap::init_assume_zeroed(buf.add(hm_start), hm_layout)
            },
            hyperlink_set: HyperlinkSet {
                refs: buf.member(refs_start),
            },
        }
    }

    fn memory_ptr(&self) -> *const u8 {
        self.memory_ptr
    }

    /// The hazard: a `&mut Page` -> `&mut [u8]` projection of the page memory.
    fn memory_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.memory_ptr as *mut u8, self.memory_len) }
    }

    fn mem(&mut self) -> *mut u8 {
        match self.mode {
            Mode::Raw => self.memory_ptr() as *mut u8,
            Mode::MutRef => self.memory_mut().as_mut_ptr(),
        }
    }

    fn cell(&self, x: usize) -> *mut Cell {
        unsafe { self.cells.ptr(self.memory_ptr).add(x) }
    }

    // page_impl.rs `set_hyperlink_inner` (page.zig `setHyperlink`).
    fn set_hyperlink(&mut self, cell: *mut Cell, id: Id) {
        let cell_offset = unsafe { get_offset::<Cell, _>(mem!(self), cell) };
        let mut map = {
            let m = mem!(self);
            unsafe { self.hyperlink_map.map(m) }
        };
        let actx = AutoContext::default();
        let gop = map.get_or_put_context(cell_offset, &actx).unwrap();

        if gop.found_existing {
            // Always release the old hyperlink, ...
            let old = unsafe { *gop.value_ptr };
            let base = mem!(self);
            unsafe { self.hyperlink_set.release(base, old) };

            // If the hyperlink matches then we don't need to do anything.
            if old == id {
                unsafe { (*cell).hyperlink = true };
                return;
            }
        }

        // Set the hyperlink on the cell and in the map.
        unsafe { *gop.value_ptr = id };
        unsafe { (*cell).hyperlink = true };
    }

    // page_impl.rs `clear_hyperlink_inner` (page.zig `clearHyperlink`).
    fn clear_hyperlink(&mut self, cell: *mut Cell) {
        // Get our ID
        let cell_offset = unsafe { get_offset::<Cell, _>(mem!(self), cell) };
        let mut map = {
            let m = mem!(self);
            unsafe { self.hyperlink_map.map(m) }
        };
        let actx = AutoContext::default();
        let entry = match map.get_entry_context(cell_offset, &actx) {
            Some(e) => e,
            None => return,
        };

        // Release our usage of this, free memory, unset flag
        let value = unsafe { *entry.value_ptr };
        let base = mem!(self);
        unsafe { self.hyperlink_set.release(base, value) };
        unsafe { map.remove_by_ptr_context(entry.key_ptr, &actx) };
        unsafe { (*cell).hyperlink = false };
    }

    // page_impl.rs `move_hyperlink` (page.zig `moveHyperlink`).
    fn move_hyperlink(&mut self, src: *const Cell, dst: *const Cell) {
        let src_offset = unsafe { get_offset::<Cell, _>(mem!(self), src) };
        let dst_offset = unsafe { get_offset::<Cell, _>(mem!(self), dst) };
        let mut map = {
            let m = mem!(self);
            unsafe { self.hyperlink_map.map(m) }
        };
        let actx = AutoContext::default();
        let entry = map.get_entry_context(src_offset, &actx).unwrap();
        let value = unsafe { *entry.value_ptr };
        unsafe { map.remove_by_ptr_context(entry.key_ptr, &actx) };
        map.put_assume_capacity_no_clobber_context(dst_offset, value, &actx);
    }

    // page_impl.rs `set_graphemes_inner` (page.zig `setGraphemes`).
    fn set_graphemes(&mut self, cell: *mut Cell, cps: &[u32]) {
        let cell_offset = unsafe { get_offset::<Cell, _>(mem!(self), cell) };
        let mut map = {
            let m = mem!(self);
            unsafe { self.grapheme_map.map(m) }
        };
        let actx = AutoContext::default();
        let stored_cps = &cps[..cps.len().min(CHUNK)];

        let base = mem!(self);
        let slice = unsafe { self.grapheme_alloc.alloc(base, stored_cps.len()) };
        slice.copy_from_slice(stored_cps);

        // `slice.ptr` / `slice.len`: the raw fat-pointer halves, read before
        // the next `self.memory` is evaluated (Zig reads them without a
        // reborrow of the elements).
        let (ptr, len) = (slice.as_ptr(), slice.len());
        let value = OffsetSlice {
            offset: unsafe { get_offset::<u32, _>(mem!(self), ptr) },
            len,
        };
        map.put_no_clobber_context(cell_offset, value, &actx)
            .unwrap();

        unsafe { (*cell).grapheme = true };
    }

    // page_impl.rs `append_grapheme_inner` (page.zig `appendGrapheme`), both
    // the no-graphemes-yet arm and the spare-chunk-space arm.
    fn append_grapheme(&mut self, cell: *mut Cell, cp: u32) {
        let cell_offset = unsafe { get_offset::<Cell, _>(mem!(self), cell) };
        let mut map = {
            let m = mem!(self);
            unsafe { self.grapheme_map.map(m) }
        };
        let actx = AutoContext::default();

        // If this cell has no graphemes, we can go faster by knowing we
        // need to allocate a new grapheme slice and update the map.
        if !unsafe { (*cell).grapheme } {
            let base = mem!(self);
            let cps = unsafe { self.grapheme_alloc.alloc(base, 1) };
            cps[0] = cp;

            let value = OffsetSlice {
                // `@ptrCast(cps.ptr)`, as in `set_graphemes`.
                offset: {
                    let ptr = cps.as_ptr();
                    unsafe { get_offset::<u32, _>(mem!(self), ptr) }
                },
                len: 1,
            };
            map.put_no_clobber_context(cell_offset, value, &actx)
                .unwrap();

            unsafe { (*cell).grapheme = true };
            return;
        }

        let slice = map.get_ptr_context(cell_offset, &actx).unwrap();
        let slice_ref = unsafe { &mut *slice };

        // If our slice len doesn't divide evenly by the grapheme chunk
        // length then we can utilize the additional chunk space.
        assert!(slice_ref.len % CHUNK != 0);
        let cps = unsafe { slice_ref.offset.ptr(mem!(self)) };
        unsafe { *cps.add(slice_ref.len) = cp };
        slice_ref.len += 1;
    }

    fn hyperlink_count(&mut self) -> u32 {
        {
            let m = mem!(self);
            unsafe { self.hyperlink_map.map(m) }
        }
        .count()
    }

    fn lookup_hyperlink(&mut self, cell: *const Cell) -> Option<Id> {
        let cell_offset = unsafe { get_offset::<Cell, _>(mem!(self), cell) };
        let map = {
            let m = mem!(self);
            unsafe { self.hyperlink_map.map(m) }
        };
        map.get_context(cell_offset, &AutoContext::default())
    }

    fn lookup_grapheme(&mut self, cell: *const Cell) -> Option<Vec<u32>> {
        let cell_offset = unsafe { get_offset::<Cell, _>(mem!(self), cell) };
        let map = {
            let m = mem!(self);
            unsafe { self.grapheme_map.map(m) }
        };
        let s = map.get_context(cell_offset, &AutoContext::default())?;
        let p = unsafe { s.offset.ptr(mem!(self)) };
        Some((0..s.len).map(|i| unsafe { *p.add(i) }).collect())
    }

    fn deinit(self) {
        let layout = std::alloc::Layout::from_size_align(self.memory_len, 64).unwrap();
        unsafe { std::alloc::dealloc(self.memory_ptr as *mut u8, layout) };
    }
}

/// Every scenario seeds with the RAW derivation so only the operation under
/// test differs between the modes.
fn seeded(mode: Mode) -> Page {
    let mut page = Page::init(Mode::Raw);
    let base = page.memory_ptr() as *mut u8;
    for id in 1..SET_CAP as Id {
        unsafe { page.hyperlink_set.use_id(base, id) };
        unsafe { page.hyperlink_set.use_id(base, id) };
    }
    page.set_hyperlink(page.cell(1), 3);
    page.set_hyperlink(page.cell(2), 4);
    page.set_graphemes(page.cell(5), &[0x301]);
    page.mode = mode;
    page
}

fn run(mode: Mode, scenario: &str) {
    let mut page = seeded(mode);
    match scenario {
        // found_existing: gop -> set release -> write through gop.value_ptr
        "set_hyperlink" => {
            page.set_hyperlink(page.cell(1), 5);
            page.mode = Mode::Raw;
            assert_eq!(Some(5), page.lookup_hyperlink(page.cell(1)));
            assert_eq!(2, page.hyperlink_count());
        }
        // entry -> set release -> removeByPtr through the cached keys
        "clear_hyperlink" => {
            page.clear_hyperlink(page.cell(1));
            page.mode = Mode::Raw;
            assert_eq!(None, page.lookup_hyperlink(page.cell(1)));
            assert_eq!(Some(4), page.lookup_hyperlink(page.cell(2)));
            assert_eq!(1, page.hyperlink_count());
        }
        // one map value, no page-memory access in between (control)
        "move_hyperlink" => {
            page.move_hyperlink(page.cell(2), page.cell(9));
            page.mode = Mode::Raw;
            assert_eq!(None, page.lookup_hyperlink(page.cell(2)));
            assert_eq!(Some(4), page.lookup_hyperlink(page.cell(9)));
        }
        // map -> grapheme alloc -> getOffset -> putNoClobber
        "set_graphemes" => {
            page.set_graphemes(page.cell(7), &[0x302, 0x303]);
            page.mode = Mode::Raw;
            assert_eq!(Some(vec![0x302, 0x303]), page.lookup_grapheme(page.cell(7)));
            assert_eq!(Some(vec![0x301]), page.lookup_grapheme(page.cell(5)));
        }
        // no-graphemes arm (alloc between map and put), then the
        // spare-chunk arm (getPtr -> cps via base -> write through the slice)
        "append_grapheme" => {
            page.append_grapheme(page.cell(8), 0x304);
            page.append_grapheme(page.cell(5), 0x305);
            page.mode = Mode::Raw;
            assert_eq!(Some(vec![0x304]), page.lookup_grapheme(page.cell(8)));
            assert_eq!(Some(vec![0x301, 0x305]), page.lookup_grapheme(page.cell(5)));
        }
        // A map value handed around by value (the reason `Header` keeps the
        // size counter in the buffer, `6112935a2`) and held across a page
        // mutation of the SAME map: the terminal test "VS16 widening when
        // the spacer tail grows the page" and the inspector page widget read
        // `hyperlink_map.map(page.memory)` around page writes.
        "held_across" => {
            let held = {
                let m = page.mem();
                unsafe { page.hyperlink_map.map(m) }
            };
            let before = held.count();
            page.set_hyperlink(page.cell(3), 6);
            assert_eq!(before + 1, held.count());
            let off = unsafe { get_offset::<Cell, _>(page.memory_ptr(), page.cell(3)) };
            assert_eq!(Some(6), held.get_context(off, &AutoContext::default()));
        }
        other => panic!("unknown scenario {other}"),
    }
    page.deinit();
}

fn main() {
    let arg = std::env::args()
        .nth(1)
        .expect("usage: mirihar-p7-hashmap <mode>_<scenario>");
    let (mode, scenario) = if let Some(s) = arg.strip_prefix("raw_") {
        (Mode::Raw, s)
    } else if let Some(s) = arg.strip_prefix("mutref_") {
        (Mode::MutRef, s)
    } else {
        panic!("unknown candidate {arg}");
    };
    run(mode, scenario);
    println!("{arg}: ok");
}
