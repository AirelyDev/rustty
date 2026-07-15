//! Isolated, non-FFI Miri harness for the vxfw (`cli +boo`) raw-aliasing hazards.
//!
//! It faithfully models the SHAPES the real `rustty_vaxis::vxfw` + `cli/boo.rs` use,
//! stripped of the tty/zlib FFI (Miri cannot execute foreign fns). The three proof
//! obligations:
//!
//!  H1 — type-erased `Widget` re-entry: `*mut Boo` lives in the Widget's
//!       `userdata: *mut c_void`, cast back to drive `draw`/`update_frame` per frame,
//!       interleaved with a `Surface` that holds a RAW alias into `Boo.buffer`. The
//!       correct shape recovers `self` as a raw `*mut Boo` and writes cells through the
//!       buffer's OWN data pointer with a narrow projection — never a whole `&mut Boo`
//!       spanning the outstanding buffer alias.
//!
//!  H2 — `App.mouse_handler.last_frame = surface` whose `children` (and their buffers)
//!       point into the arena, then `arena.reset()` (`.free_all`) before the next
//!       `do_layout`. The `Surface` slices are RAW (`*mut [T]`), so storing the stale
//!       `last_frame` across `reset` is holding dangling RAW pointers — never
//!       dereferenced after the reset (the fresh frame overwrites `last_frame` before
//!       any read). We model draw → store → reset → realloc → render and read ONLY the
//!       fresh frame.
//!
//!  H3 — the `Boo` child `Surface.buffer = &self.buffer` raw alias vs the next tick's
//!       `update_frame` writing `self.buffer[i]`. We prove temporal soundness by
//!       writing through the buffer's own raw data pointer (`Vec::as_mut_ptr` — a
//!       narrow `&mut Vec` retag, NOT a `&mut [Cell]` over the heap the alias covers),
//!       so the stored alias and the writes are compatible raw tags.
//!
//! Run under BOTH aliasing models:
//!   cargo +nightly miri run                        # Stacked Borrows (default)
//!   MIRIFLAGS="-Zmiri-tree-borrows" cargo +nightly miri run
//! (`cargo +nightly miri test` runs the `#[test]`s in whichever model is selected.)
#![allow(dead_code, unused_assignments)]

use std::alloc::{alloc, dealloc, Layout};
use std::cell::UnsafeCell;
use std::os::raw::c_void;

// ===========================================================================
// A bump arena with reset(.free_all) — the faithful image of VxfwArena.
// ===========================================================================

struct Arena {
    inner: UnsafeCell<ArenaInner>,
}
struct ArenaInner {
    chunks: Vec<(*mut u8, Layout)>,
    off: usize,
}
const CHUNK: usize = 4096;

impl Arena {
    fn new() -> Self {
        Arena {
            inner: UnsafeCell::new(ArenaInner {
                chunks: Vec::new(),
                off: 0,
            }),
        }
    }
    /// alloc `n` `T` slots (uninitialized) — raw fat pointer, caller initializes.
    unsafe fn alloc_slice<T: Copy>(&self, n: usize) -> *mut [T] {
        let size = std::mem::size_of::<T>() * n;
        if size == 0 {
            return std::ptr::slice_from_raw_parts_mut(
                std::ptr::NonNull::<T>::dangling().as_ptr(),
                n,
            );
        }
        let align = std::mem::align_of::<T>();
        let inner = unsafe { &mut *self.inner.get() };
        let ptr = inner.bump(size, align);
        std::ptr::slice_from_raw_parts_mut(ptr.cast::<T>(), n)
    }
    fn reset(&mut self) {
        let inner = self.inner.get_mut();
        for (p, l) in inner.chunks.drain(..) {
            unsafe { dealloc(p, l) };
        }
        inner.off = 0;
    }
}
impl ArenaInner {
    fn bump(&mut self, size: usize, align: usize) -> *mut u8 {
        if let Some(&(start, layout)) = self.chunks.last() {
            let misalign = (start as usize).wrapping_add(self.off) & (align - 1);
            let pad = if misalign == 0 { 0 } else { align - misalign };
            let aligned = self.off + pad;
            if aligned + size <= layout.size() {
                self.off = aligned + size;
                return unsafe { start.add(aligned) };
            }
        }
        let cs = size.max(CHUNK);
        let ca = align.max(16);
        let layout = Layout::from_size_align(cs, ca).unwrap();
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null());
        self.chunks.push((ptr, layout));
        self.off = size;
        ptr
    }
}
impl Drop for Arena {
    fn drop(&mut self) {
        self.reset();
    }
}

// ===========================================================================
// Minimal vxfw shapes: Cell / Widget / Surface (RAW slices) / SubSurface.
// ===========================================================================

#[derive(Clone, Copy)]
struct Cell {
    // stand-in for vaxis.Cell: a byte payload + a "grapheme id" borrowing frame data.
    byte: u8,
    style: u8,
}
impl Default for Cell {
    fn default() -> Self {
        Cell { byte: 0, style: 0 }
    }
}

type DrawFn = fn(*mut c_void, *const Arena) -> Surface;

#[derive(Clone, Copy)]
struct Widget {
    userdata: *mut c_void,
    draw_fn: DrawFn,
    has_handler: bool,
}
impl Widget {
    fn draw(self, arena: *const Arena) -> Surface {
        (self.draw_fn)(self.userdata, arena)
    }
    fn eql(self, other: Widget) -> bool {
        self.userdata as usize == other.userdata as usize
            && self.draw_fn as usize == other.draw_fn as usize
    }
}

#[derive(Clone, Copy)]
struct Surface {
    w: u16,
    h: u16,
    widget: Widget,
    buffer: *mut [Cell],
    children: *mut [SubSurface],
}

#[derive(Clone, Copy)]
struct SubSurface {
    row: u16,
    col: u16,
    surface: Surface,
}

fn empty_cells() -> *mut [Cell] {
    std::ptr::slice_from_raw_parts_mut(std::ptr::NonNull::<Cell>::dangling().as_ptr(), 0)
}
fn empty_children() -> *mut [SubSurface] {
    std::ptr::slice_from_raw_parts_mut(std::ptr::NonNull::<SubSurface>::dangling().as_ptr(), 0)
}

impl Surface {
    /// render: read every cell through the RAW buffer alias (the H1/H3 read side).
    fn render(self, sink: &mut u64) {
        if self.buffer.len() > 0 {
            let buf = unsafe { &*self.buffer };
            for c in buf {
                *sink = sink.wrapping_add(c.byte as u64 + c.style as u64);
            }
        }
        // sort children by a trivial key (models Surface.render's in-place sort of the
        // arena-backed children through a transient &mut — H2 read/mutate side).
        let children = unsafe { &mut *self.children };
        children.sort_by(|a, b| a.col.cmp(&b.col));
        for ch in children.iter() {
            ch.surface.render(sink);
        }
    }

    /// hit_test: walk children (RAW read) collecting handler widgets (H2 read side).
    fn hit_test(self, list: &mut Vec<Widget>) {
        if self.widget.has_handler {
            list.push(self.widget);
        }
        let children = unsafe { &*self.children };
        for ch in children {
            ch.surface.hit_test(list);
        }
    }
}

// ===========================================================================
// Boo — the type-erased widget with the aliased buffer (H1/H3).
// ===========================================================================

const W: usize = 4;
const H: usize = 3;
const N: usize = W * H; // 12 cells (stands in for 100x41=4100)

struct Boo<'a> {
    frame: u8,
    buffer: Vec<Cell>,
    frames: &'a [&'a [u8]],
}

impl<'a> Boo<'a> {
    fn widget_from_ptr(ptr: *mut Boo<'a>) -> Widget {
        Widget {
            userdata: ptr as *mut c_void,
            draw_fn: Boo::draw,
            has_handler: true,
        }
    }

    /// H3: raw-self write through the buffer's OWN data pointer. NEVER `&mut Boo`
    /// spanning the outstanding `last_frame` buffer alias.
    unsafe fn update_frame(ptr: *mut Boo<'a>) {
        let frames = unsafe { (*ptr).frames };
        let idx = unsafe { (*ptr).frame } as usize;
        let frame = frames[idx];
        // explicit &mut Vec (narrow field retag) → raw data ptr (NOT &mut [Cell]).
        let buf_ref: &mut Vec<Cell> = unsafe { &mut (*ptr).buffer };
        let data = buf_ref.as_mut_ptr();
        let len = buf_ref.len();
        let mut cell_idx = 0usize;
        for &b in frame {
            if b == b'\n' {
                continue;
            }
            assert!(cell_idx < len);
            unsafe {
                data.add(cell_idx).write(Cell { byte: b, style: 1 });
            }
            cell_idx += 1;
        }
        // pad the remainder so we always fill the buffer (models the 4100 assert).
        while cell_idx < len {
            unsafe {
                data.add(cell_idx).write(Cell {
                    byte: b' ',
                    style: 0,
                })
            };
            cell_idx += 1;
        }
        assert!(cell_idx == len);
        let flen = frames.len();
        unsafe {
            (*ptr).frame += 1;
            if (*ptr).frame as usize == flen {
                (*ptr).frame = 0;
            }
        }
    }

    /// H1: draw recovers `self` as a raw `*mut Boo`, builds a child Surface whose
    /// buffer is a RAW alias into `self.buffer` (via `Vec::as_ptr`, no `&[Cell]`),
    /// and an arena-allocated child slice.
    fn draw(ptr: *mut c_void, arena: *const Arena) -> Surface {
        let self_ptr = ptr as *mut Boo;
        let data = unsafe { (*self_ptr).buffer.as_ptr() as *mut Cell };
        let buffer_alias = std::ptr::slice_from_raw_parts_mut(data, N);
        let child = Surface {
            w: W as u16,
            h: H as u16,
            widget: Boo::widget_from_ptr(self_ptr),
            buffer: buffer_alias,
            children: empty_children(),
        };
        let children = unsafe { (*arena).alloc_slice::<SubSurface>(1) };
        unsafe {
            children.cast::<SubSurface>().write(SubSurface {
                row: 0,
                col: 0,
                surface: child,
            });
        }
        Surface {
            w: 10,
            h: 10,
            widget: Boo::widget_from_ptr(self_ptr),
            buffer: empty_cells(),
            children,
        }
    }
}

// ===========================================================================
// The in-memory loop: feed Init/Tick/KeyPress, drive the full interleave.
// ===========================================================================

enum Event {
    Init,
    Tick,
    KeyPress(u8),
}

/// last_frame holder (models App.mouse_handler.last_frame across arena.reset — H2).
struct MouseHandler {
    last_frame: Surface,
}

fn run_interleave() -> u64 {
    // Boo over stub frames (no zlib): 3 frames of ascii, each < N printable bytes.
    let frames: Vec<&[u8]> = vec![b"abc\ndef", b"ghi\njkl", b"mno\npqr"];
    let mut boo = Boo {
        frame: 0,
        buffer: vec![Cell::default(); N],
        frames: &frames,
    };
    let boo_ptr: *mut Boo = &mut boo;
    let root = Boo::widget_from_ptr(boo_ptr);

    let mut arena = Arena::new();
    let mut mh = MouseHandler {
        last_frame: Surface {
            w: 0,
            h: 0,
            widget: root,
            buffer: empty_cells(),
            children: empty_children(),
        },
    };

    let events = [
        Event::Init,
        Event::Tick,
        Event::KeyPress(b'x'),
        Event::Tick,
        Event::Tick,
        Event::KeyPress(b'c'),
    ];

    let mut sink: u64 = 0u64;
    let mut quit = false;

    for ev in events {
        if quit {
            break;
        }
        // handle event (the type-erased handler path): Init/Tick drive update_frame
        // via the RAW self ptr (H1/H3); KeyPress('c') quits.
        match ev {
            Event::Init | Event::Tick => {
                // SAFETY (H1/H3): raw-self update through the buffer's own ptr while
                // mh.last_frame may still hold a stale RAW alias into boo.buffer.
                unsafe { Boo::update_frame(boo_ptr) };
            }
            Event::KeyPress(k) => {
                if k == b'c' {
                    quit = true;
                    continue;
                }
            }
        }

        // redraw:
        // H2: reset the arena while mh.last_frame still holds the PREVIOUS frame's
        // arena-backed children (dangling RAW pointers). We never deref last_frame
        // after this point — the fresh surface overwrites it before any read.
        arena.reset();

        // do_layout → fresh surface aliasing boo.buffer + fresh arena children.
        let surface = root.draw(&arena);

        // hit_test the FRESH surface (never the pre-reset last_frame). Exercise the
        // type-erased `eql` walk (fn-ptr identity is Miri-synthetic, so we assert on
        // the hit-list shape, not the equality result).
        let mut hits: Vec<Widget> = Vec::new();
        surface.hit_test(&mut hits);
        let _matched = hits.iter().any(|w| w.eql(root));
        assert!(!hits.is_empty());

        // store last_frame (H2: stale after next reset) then render (reads the buffer
        // alias — the H1/H3 read side, interleaved with the next tick's writes).
        mh.last_frame = surface;
        surface.render(&mut sink);
    }

    // teardown: arena drops (frees remaining chunks); boo drops (frees buffer). Any
    // dangling last_frame RAW pointers are never dereferenced again.
    sink
}

fn main() {
    let s = run_interleave();
    println!("sink={s}");
    assert!(s > 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    // H1+H2+H3 together: under Stacked AND Tree Borrows, the type-erased re-entry, the
    // last_frame-across-reset, and the buffer-alias-vs-raw-write interleave must not
    // trip any borrow-stack / tree-borrows violation. Miri interprets the whole run.
    #[test]
    fn vxfw_raw_aliasing_is_sound() {
        let s = run_interleave();
        assert!(s > 0);
    }

    // The buffer alias stored in a Surface survives an arena reset as a dangling RAW
    // pointer (H2). Storing + overwriting it (never dereferencing post-reset) is sound
    // precisely because the slices are `*mut [T]`, not `&'a [T]`.
    #[test]
    fn last_frame_raw_pointers_survive_reset_without_deref() {
        let frames: Vec<&[u8]> = vec![b"abc\ndef"];
        let mut boo = Boo {
            frame: 0,
            buffer: vec![Cell::default(); N],
            frames: &frames,
        };
        let boo_ptr: *mut Boo = &mut boo;
        let root = Boo::widget_from_ptr(boo_ptr);
        let mut arena = Arena::new();

        unsafe { Boo::update_frame(boo_ptr) };
        let stale = root.draw(&arena); // children in arena
                                       // reset frees the arena chunk `stale.children` points at — a dangling RAW ptr.
        arena.reset();
        // We do NOT deref `stale` after reset. A fresh draw reuses the freed memory.
        unsafe { Boo::update_frame(boo_ptr) };
        let fresh = root.draw(&arena);
        let mut sink = 0u64;
        fresh.render(&mut sink);
        // `stale` is simply dropped (Copy) — never dereferenced. Sound.
        let _ = stale;
        assert!(sink > 0);
    }
}
