//! Isolated, non-FFI Miri harness for the borrow shapes parity tail S7-6 added to
//! the vt search (`crates/rustty-vt/src/search/*`, `highlight.rs`, `zig_list.rs`),
//! which the real tests (rustty-vt links C libraries) cannot always run under Miri.
//! Each candidate is one executed path, selected by argv[1] (Miri only checks
//! executed code). The functions below are the port's own code, copied with the
//! allocator handle replaced by allocator_api2's `Global` (the handle's Global
//! arm forwards to exactly that); the element types are reduced to what the
//! borrow shapes need. See README.md.
#![allow(dead_code)]

use allocator_api2::alloc::{Allocator as ApiAllocator, Global};
use allocator_api2::vec::Vec as AVec;
use core::alloc::Layout;
use core::ptr::NonNull;

// ---------------------------------------------------------------------------
// zig_list.rs (verbatim but for the allocator type)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
struct OutOfMemory;

const fn grow_capacity(minimum: usize, init_capacity: usize) -> usize {
    minimum.saturating_add(minimum / 2 + init_capacity)
}

fn ensure_total_capacity_precise<T>(v: &mut AVec<T, Global>, n: usize) -> Result<(), OutOfMemory> {
    if v.capacity() >= n {
        return Ok(());
    }
    v.try_reserve_exact(n - v.len()).map_err(|_| OutOfMemory)
}

fn ensure_total_capacity<T>(v: &mut AVec<T, Global>, n: usize, init: usize) -> Result<(), OutOfMemory> {
    if v.capacity() >= n {
        return Ok(());
    }
    ensure_total_capacity_precise(v, grow_capacity(n, init))
}

fn ensure_unused_capacity<T>(v: &mut AVec<T, Global>, n: usize, init: usize) -> Result<(), OutOfMemory> {
    let total = v.len().checked_add(n).ok_or(OutOfMemory)?;
    ensure_total_capacity(v, total, init)
}

#[inline(always)]
fn append_n_times<T: Copy>(v: &mut AVec<T, Global>, value: T, n: usize, init: usize) -> Result<(), OutOfMemory> {
    if v.capacity() - v.len() < n {
        ensure_unused_capacity(v, n, init)?;
    }
    let len = v.len();
    unsafe {
        let p = v.as_mut_ptr().add(len);
        for i in 0..n {
            p.add(i).write(value);
        }
        v.set_len(len + n);
    }
    Ok(())
}

fn shrink_and_free<T>(v: &mut AVec<T, Global>, new_len: usize) {
    v.truncate(new_len);
    if core::mem::size_of::<T>() == 0 || v.capacity() == new_len {
        return;
    }
    let alloc = *v.allocator();
    let taken = core::mem::replace(v, AVec::new_in(alloc));
    let (ptr, len, cap, alloc) = taken.into_raw_parts_with_alloc();
    let old = Layout::array::<T>(cap).unwrap();
    let new = Layout::array::<T>(new_len).unwrap();
    let ptr_u8 = unsafe { NonNull::new_unchecked(ptr as *mut u8) };
    match unsafe { ApiAllocator::shrink(&alloc, ptr_u8, old, new) } {
        Ok(p) => *v = unsafe { AVec::from_raw_parts_in(p.as_ptr() as *mut T, len, new_len, alloc) },
        Err(_) => *v = unsafe { AVec::from_raw_parts_in(ptr, len, cap, alloc) },
    }
}

fn set_capacity_alloc_copy_free<T: Copy>(v: &mut AVec<T, Global>, n: usize) -> Result<(), OutOfMemory> {
    let mut fresh: AVec<T, Global> = AVec::new_in(*v.allocator());
    fresh.try_reserve_exact(n).map_err(|_| OutOfMemory)?;
    unsafe {
        core::ptr::copy_nonoverlapping(v.as_ptr(), fresh.as_mut_ptr(), v.len());
        fresh.set_len(v.len());
    }
    *v = fresh;
    Ok(())
}

fn multi_clone<T: Copy>(src: &[T], init: usize) -> Result<AVec<T, Global>, OutOfMemory> {
    let mut out: AVec<T, Global> = AVec::new_in(Global);
    if out.capacity() < src.len() {
        set_capacity_alloc_copy_free(&mut out, grow_capacity(src.len(), init))?;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), out.as_mut_ptr(), src.len());
        out.set_len(src.len());
    }
    Ok(out)
}

struct AllocatingWriter {
    buf: AVec<u8, Global>,
}

impl std::io::Write for AllocatingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.buf.capacity() - self.buf.len() {
            let extra = bytes.len() * 2 + 1;
            ensure_unused_capacity(&mut self.buf, extra, 128)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::OutOfMemory))?;
        }
        let len = self.buf.len();
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), self.buf.as_mut_ptr().add(len), bytes.len());
            self.buf.set_len(len + bytes.len());
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// highlight.rs: Flattened<C> / FlattenedRef
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
struct Chunk {
    node: *mut u8,
    serial: u64,
    start: u16,
    end: u16,
}

struct Flattened<C = AVec<Chunk, Global>> {
    chunks: C,
    top_x: u16,
    bot_x: u16,
}
type FlattenedRef<'a> = Flattened<&'a [Chunk]>;

impl<C: core::ops::Deref<Target = [Chunk]>> Flattened<C> {
    fn clone_in(&self) -> Result<Flattened, OutOfMemory> {
        Ok(Flattened {
            chunks: multi_clone(&self.chunks, 16)?,
            top_x: self.top_x,
            bot_x: self.bot_x,
        })
    }
}

impl Flattened {
    fn as_ref(&self) -> FlattenedRef<'_> {
        Flattened {
            chunks: &self.chunks[..],
            top_x: self.top_x,
            bot_x: self.bot_x,
        }
    }
}

// ---------------------------------------------------------------------------
// sliding_window.rs: the private CircBuf (the parts `next`/`highlight` touch)
// ---------------------------------------------------------------------------

struct CircBuf {
    storage: AVec<u8, Global>,
    head: usize,
    tail: usize,
    full: bool,
}

impl CircBuf {
    fn init() -> CircBuf {
        CircBuf { storage: AVec::new_in(Global), head: 0, tail: 0, full: true }
    }
    fn len(&self) -> usize {
        if self.full {
            return self.storage.len();
        }
        if self.head >= self.tail {
            return self.head - self.tail;
        }
        self.storage.len() - (self.tail - self.head)
    }
    fn capacity(&self) -> usize {
        self.storage.len()
    }
    fn rotate_to_zero(&mut self) {
        if self.tail == 0 {
            return;
        }
        self.storage.rotate_left(self.tail);
        self.head = self.len() % self.storage.len();
        self.tail = 0;
    }
    fn resize(&mut self, size: usize) -> Result<(), OutOfMemory> {
        assert!(size >= self.storage.len());
        self.rotate_to_zero();
        let prev_len = self.len();
        let prev_cap = self.storage.len();
        ensure_total_capacity_precise(&mut self.storage, size)?;
        let retained_len = core::cmp::min(prev_len, size);
        for _ in prev_cap..size {
            self.storage.push(0);
        }
        self.head = if retained_len == size { 0 } else { retained_len };
        self.tail = 0;
        self.full = retained_len == size;
        Ok(())
    }
    fn ensure_unused_capacity(&mut self, amount: usize) -> Result<(), OutOfMemory> {
        let new_cap = self.len() + amount;
        if new_cap <= self.capacity() {
            return Ok(());
        }
        self.resize(new_cap)
    }
    fn advance(&mut self, amount: usize) {
        self.head += amount;
        if self.head >= self.storage.len() {
            self.head -= self.storage.len();
        }
        if self.full {
            self.tail = self.head;
        }
        self.full = self.head == self.tail;
    }
    fn storage_offset(&self, offset: usize) -> usize {
        let fits = self.tail + offset;
        if fits < self.storage.len() {
            fits
        } else {
            fits - self.storage.len()
        }
    }
    fn get_ptr_slice(&mut self, offset: usize, slice_len: usize) -> (*mut [u8], *mut [u8]) {
        if slice_len == 0 {
            let empty: &mut [u8] = &mut [];
            return (empty as *mut [u8], &mut [] as *mut [u8]);
        }
        let end_offset = offset + slice_len;
        if end_offset > self.len() {
            self.advance(end_offset - self.len());
        }
        let start_idx = self.storage_offset(offset);
        let end_idx = self.storage_offset(end_offset - 1);
        if end_idx >= start_idx {
            let (left, right) = self.storage.split_at_mut(end_idx + 1);
            let single: *mut [u8] = &mut left[start_idx..];
            let empty: *mut [u8] = &mut right[0..0];
            return (single, empty);
        }
        let (head, tail) = self.storage.split_at_mut(start_idx);
        let first: *mut [u8] = tail;
        let second: *mut [u8] = &mut head[0..end_idx + 1];
        (first, second)
    }
    fn append_slice_assume_capacity(&mut self, slice: &[u8]) {
        let storage = self.get_ptr_slice(self.len(), slice.len());
        unsafe {
            let s0 = &mut *storage.0;
            let s1 = &mut *storage.1;
            let n0 = s0.len();
            s0.copy_from_slice(&slice[0..n0]);
            s1.copy_from_slice(&slice[n0..]);
        }
    }
    fn delete_oldest(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let (s0, s1) = self.get_ptr_slice(0, n);
        unsafe {
            for v in (*s0).iter_mut() {
                *v = 0;
            }
            for v in (*s1).iter_mut() {
                *v = 0;
            }
        }
        self.tail += core::cmp::min(self.len(), n);
        if self.tail >= self.storage.len() {
            self.tail -= self.storage.len();
        }
        self.full = false;
    }
}

// ---------------------------------------------------------------------------
// The window: `next()` reads two raw subslices of the data buffer, then calls
// `highlight(&mut self)` (a two-phase `&mut self` whose argument reads a
// subslice), which refills `chunk_buf` and returns a borrow of it.
// ---------------------------------------------------------------------------

struct Window {
    data: CircBuf,
    chunk_buf: AVec<Chunk, Global>,
    needle: AVec<u8, Global>,
    overlap_buf: AVec<u8, Global>,
    data_offset: usize,
}

impl Window {
    fn init(needle: &[u8]) -> Window {
        let mut n = AVec::new_in(Global);
        n.try_reserve_exact(needle.len()).unwrap();
        for &b in needle {
            n.push(b);
        }
        let mut o = AVec::new_in(Global);
        o.try_reserve_exact(needle.len() * 2).unwrap();
        for _ in 0..needle.len() * 2 {
            o.push(0);
        }
        Window { data: CircBuf::init(), chunk_buf: AVec::new_in(Global), needle: n, overlap_buf: o, data_offset: 0 }
    }

    fn append(&mut self, text: &[u8]) -> Result<(), OutOfMemory> {
        // encode through the allocating writer, then copy into the ring
        let mut encoded = AllocatingWriter { buf: AVec::new_in(Global) };
        std::io::Write::write_all(&mut encoded, text).map_err(|_| OutOfMemory)?;
        self.data.ensure_unused_capacity(encoded.buf.len())?;
        if self.chunk_buf.capacity() < 4 {
            set_capacity_alloc_copy_free(&mut self.chunk_buf, grow_capacity(4, 16))?;
        }
        self.data.append_slice_assume_capacity(&encoded.buf);
        Ok(())
    }

    fn next(&mut self) -> Option<FlattenedRef<'_>> {
        if self.needle.is_empty() {
            return None;
        }
        let (slice0, slice1): (&[u8], &[u8]) = {
            let data_len = self.data.len();
            if data_len < self.needle.len() {
                return None;
            }
            let (s0, s1) = self.data.get_ptr_slice(self.data_offset, data_len - self.data_offset);
            unsafe { (&*s0, &*s1) }
        };
        if let Some(idx) = find(slice0, &self.needle) {
            return Some(self.highlight(idx, self.needle.len()));
        }
        if !slice0.is_empty() && !slice1.is_empty() {
            let len0 = core::cmp::min(slice0.len(), self.needle.len() - 1);
            let prefix = &slice0[slice0.len() - len0..];
            let len1 = core::cmp::min(slice1.len(), self.needle.len() - 1);
            let suffix = &slice1[0..len1];
            let overlap_len = prefix.len() + suffix.len();
            self.overlap_buf[0..prefix.len()].copy_from_slice(prefix);
            self.overlap_buf[prefix.len()..overlap_len].copy_from_slice(suffix);
            if let Some(idx) = find(&self.overlap_buf[0..overlap_len], &self.needle) {
                return Some(self.highlight(slice0.len() - prefix.len() + idx, self.needle.len()));
            }
        }
        if let Some(idx) = find(slice1, &self.needle) {
            return Some(self.highlight(slice0.len() + idx, self.needle.len()));
        }
        // prune everything but needle.len - 1
        let keep = self.needle.len() - 1;
        let len = self.data.len();
        if len > keep {
            self.data.delete_oldest(len - keep);
        }
        self.data_offset = 0;
        None
    }

    fn highlight(&mut self, start_offset: usize, _len: usize) -> FlattenedRef<'_> {
        self.chunk_buf.clear();
        debug_assert!(self.chunk_buf.len() < self.chunk_buf.capacity());
        self.chunk_buf.push(Chunk { node: core::ptr::null_mut(), serial: 1, start: 0, end: 1 });
        let start = start_offset + self.data_offset;
        self.data_offset = start + 1;
        // highlight() prunes data before the match (deleteOldest) in the port.
        if start > 0 {
            self.data.delete_oldest(start);
            self.data_offset = 1;
        }
        Flattened { chunks: &self.chunk_buf[..], top_x: start as u16, bot_x: 0 }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()].eq_ignore_ascii_case(needle))
}

// ---------------------------------------------------------------------------
// The screen search: `self.active.next()` borrowed while a SIBLING field grows
// (tick_active), and the history searcher inside an Option (tick_history).
// ---------------------------------------------------------------------------

struct History {
    searcher: Window,
    start_node: *mut u8,
}

struct Screenish {
    active: Window,
    history: Option<History>,
    active_results: AVec<Flattened, Global>,
    history_results: AVec<Flattened, Global>,
}

impl Screenish {
    fn tick_active(&mut self) -> Result<(), OutOfMemory> {
        while let Some(hl) = self.active.next() {
            let c = hl.clone_in()?;
            if self.active_results.len() == self.active_results.capacity() {
                ensure_unused_capacity(&mut self.active_results, 1, 4)?;
            }
            self.active_results.push(c);
        }
        Ok(())
    }
    fn tick_history(&mut self) -> Result<(), OutOfMemory> {
        let history = self.history.as_mut().unwrap();
        while let Some(hl) = history.searcher.next() {
            if core::ptr::eq(hl.chunks[0].node, history.start_node) {
                continue;
            }
            let c = hl.clone_in()?;
            if self.history_results.len() == self.history_results.capacity() {
                ensure_unused_capacity(&mut self.history_results, 1, 4)?;
            }
            self.history_results.push(c);
        }
        Ok(())
    }
    fn match_at(&self, idx: usize) -> Option<FlattenedRef<'_>> {
        let a = self.active_results.len();
        if idx < a {
            return Some(self.active_results[a - 1 - idx].as_ref());
        }
        self.history_results.get(idx - a).map(|h| h.as_ref())
    }
}

fn window_next() {
    let mut w = Window::init(b"boo!");
    w.append(b"hello. boo! hello. boo! x").unwrap();
    w.append(b"bo").unwrap();
    w.append(b"o! and more text boo!").unwrap();
    let mut n = 0;
    while let Some(h) = w.next() {
        assert_eq!(1, h.chunks.len());
        n += 1;
        assert!(n < 20);
    }
    assert!(n >= 3, "{n}");
}

fn tick_sibling() {
    let mut s = Screenish {
        active: Window::init(b"ab"),
        history: Some(History { searcher: Window::init(b"ab"), start_node: core::ptr::without_provenance_mut(0x10) }),
        active_results: AVec::new_in(Global),
        history_results: AVec::new_in(Global),
    };
    for _ in 0..3 {
        s.active.append(b"xxabxxabxxab").unwrap();
        s.history.as_mut().unwrap().searcher.append(b"ab ab ab ab").unwrap();
        s.tick_active().unwrap();
        s.tick_history().unwrap();
    }
    assert!(s.active_results.len() >= 3);
    assert!(s.history_results.len() >= 3);
    let r = s.match_at(0).unwrap();
    assert_eq!(1, r.chunks.len());
    let owned = r.clone_in().unwrap();
    assert_eq!(owned.chunks[..], s.active_results[s.active_results.len() - 1].chunks[..]);
    shrink_and_free(&mut s.history_results, 1);
    assert_eq!(1, s.history_results.capacity());
    shrink_and_free(&mut s.history_results, 0);
    assert_eq!(0, s.history_results.capacity());
}

fn list_helpers() {
    let mut v: AVec<(u16, u32), Global> = AVec::new_in(Global);
    for i in 0..40 {
        append_n_times(&mut v, (i, 0), 3, 16).unwrap();
    }
    assert_eq!(120, v.len());
    let snapshot: Vec<_> = v.iter().copied().collect();
    shrink_and_free(&mut v, 7);
    assert_eq!(&snapshot[..7], &v[..]);
    set_capacity_alloc_copy_free(&mut v, 100).unwrap();
    assert_eq!(&snapshot[..7], &v[..]);
    let mut w = AllocatingWriter { buf: AVec::new_in(Global) };
    for _ in 0..50 {
        std::io::Write::write_all(&mut w, b"0123456789").unwrap();
    }
    assert_eq!(500, w.buf.len());
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    match which.as_str() {
        "window_next" => window_next(),
        "tick_sibling" => tick_sibling(),
        "list_helpers" => list_helpers(),
        "all" => {
            window_next();
            tick_sibling();
            list_helpers();
        }
        other => panic!("unknown candidate {other}"),
    }
    println!("ok {which}");
}
