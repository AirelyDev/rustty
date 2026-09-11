//! PORT NOTE: Ports ghostty `src/lib/allocator.zig`.
//!
//! This file defines the C allocator ABI (`VTable`, `Allocator`) used by the C
//! libghostty (libgvt) APIs, plus the bridge to/from the Zig allocator
//! interface. In rustty the "Zig allocator interface" is modeled by the
//! `ZigAllocator` trait below (alloc/resize/remap/free over `*mut u8` +
//! `Alignment`), so we preserve the SAME vtable/ABI semantics exactly: the
//! extern `VTable` and `Allocator` structs keep their C layout (`#[repr(C)]`),
//! the function-pointer signatures match `callconv(.c)`, and `from_zig`/`zig`
//! round-trip a Zig allocator through the C struct, byte-identical in layout.

use std::ffi::c_void;

/// Wasm-specific allocation helpers.
pub mod wasm;

/// PORT NOTE: `std.mem.Alignment` — an enum whose integer value is the
/// log2 of the alignment. Crossing the C boundary it is a `u8`
/// (`@intFromEnum`/`@enumFromInt`). We model it as a transparent `u8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alignment(pub u8);

impl Alignment {
    pub fn from_byte_units(n: usize) -> Alignment {
        debug_assert!(n.is_power_of_two());
        Alignment(n.trailing_zeros() as u8)
    }

    pub fn to_byte_units(self) -> usize {
        1usize << self.0
    }

    /// PORT NOTE (W36-B06): `std.mem.Alignment.backward(addr)` — round `addr`
    /// down to this alignment. Used for the Zig zero-length/zero-sized
    /// sentinel pointer (`alignBackward(maxInt(usize), alignment)`).
    pub fn backward(self, addr: usize) -> usize {
        addr & !(self.to_byte_units() - 1)
    }
}

/// PORT NOTE (W36-B06): the Zig typed-helper zero-length sentinel — the
/// highest address of this alignment (`@ptrFromInt(alignment.backward(
/// maxInt(usize)))`, Zig 0.16 `std.mem.Allocator`). Non-null, aligned, and
/// dereference-free; returned by the typed helpers WITHOUT calling the raw
/// allocator so custom C allocators never observe zero-length callbacks.
fn zero_len_sentinel(alignment: Alignment) -> *mut u8 {
    alignment.backward(usize::MAX) as *mut u8
}

/// PORT NOTE: The Rust embodiment of a Zig `[]u8` slice — a `(ptr, len)` pair
/// with NO Rust aliasing/borrow guarantees (exactly like Zig). We use this
/// instead of `&mut [u8]` in the allocator interface so that freeing/realloc'ing
/// the underlying memory does not deallocate behind a live, "strongly
/// protected" Rust reference (which is Undefined Behavior under Stacked
/// Borrows). This matches Zig's `memory: []u8` byte-for-byte.
#[derive(Debug, Clone, Copy)]
pub struct ZigSlice {
    pub ptr: *mut u8,
    pub len: usize,
}

impl ZigSlice {
    pub fn new(ptr: *mut u8, len: usize) -> ZigSlice {
        ZigSlice { ptr, len }
    }
}

/// Useful alias since they're required to create Zig allocators
///
/// PORT NOTE: The reference declares `pub const ZigVTable =
/// std.mem.Allocator.VTable;` (with the doc-comment above, preserved verbatim).
/// rustty has no `std.mem.Allocator.VTable` type; the Zig allocator interface
/// (`std.mem.Allocator`) is modeled by the `ZigAllocator` trait below. `ptr`
/// (the Zig allocator's context) is `self`; the four methods mirror
/// `std.mem.Allocator.VTable`. `ret_addr`/`ra` is the return address hint
/// (unused by most allocators), preserved for ABI fidelity.
pub trait ZigAllocator {
    /// `alloc(ctx, len, alignment, ret_addr) ?[*]u8`
    fn alloc(&self, len: usize, alignment: Alignment, ra: usize) -> *mut u8;
    /// `resize(ctx, memory, alignment, new_len, ret_addr) bool`
    fn resize(&self, memory: ZigSlice, alignment: Alignment, new_len: usize, ra: usize) -> bool;
    /// `remap(ctx, memory, alignment, new_len, ret_addr) ?[*]u8`
    fn remap(&self, memory: ZigSlice, alignment: Alignment, new_len: usize, ra: usize) -> *mut u8;
    /// `free(ctx, memory, alignment, ret_addr) void`
    fn free(&self, memory: ZigSlice, alignment: Alignment, ra: usize);

    /// PORT NOTE (W36-B06): the Zig TYPED helper `allocator.alloc(u8, len)` —
    /// allocate `len` bytes at byte (1) alignment. Mirrors Zig 0.16
    /// `std.mem.Allocator.alloc`: a zero-length request short-circuits to the
    /// non-null aligned sentinel WITHOUT calling the raw allocator (custom C
    /// allocators never observe zero-length alloc callbacks). Returns a null
    /// ptr on failure (Zig returns `error.OutOfMemory`). `ret_addr` stays 0:
    /// stable Rust has no `@returnAddress()`; it is a hint per the Zig
    /// allocator contract (documented residual of W36-B06).
    fn alloc_bytes(&self, len: usize) -> *mut u8 {
        if len == 0 {
            // if (byte_count == 0) return @ptrFromInt(alignment.backward(maxInt(usize)));
            return zero_len_sentinel(Alignment(0));
        }
        self.alloc(len, Alignment(0), 0)
    }

    /// PORT NOTE (W36-B06): the Zig TYPED helper `allocator.free(memory)` for
    /// a byte (1) aligned `[]u8`. Mirrors Zig 0.16 `std.mem.Allocator.free`:
    /// a zero-length slice returns BEFORE the raw free (custom C allocators
    /// never observe zero-length free callbacks). `ret_addr` stays 0 (see
    /// `alloc_bytes`).
    fn free_bytes(&self, memory: ZigSlice) {
        if memory.len == 0 {
            // if (non_const_ptr.len == 0) return; (before rawFree)
            return;
        }
        self.free(memory, Alignment(0), 0);
    }
}

/// PORT NOTE (F-008/W12-01): Zig `error.OutOfMemory` as carried by the raw
/// create/destroy helpers below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfMemory;

/// PORT NOTE (F-008/W12-01): the Rust embodiment of Zig `alloc.create(T)` for
/// a C-resolved allocator — allocate UNINITIALIZED storage for one `T` through
/// the allocator's raw `alloc`, mapping rejection (null) to the recoverable
/// `OutOfMemory` (the reference's `alloc.create(...) catch return
/// error.OutOfMemory` arm). The caller initializes the storage with
/// `ptr::write` (Zig `ptr.* = ...`) and later releases it with `destroy_raw`.
/// Zero-sized `T` short-circuits to the aligned non-null sentinel without
/// calling the allocator, exactly like Zig's `Allocator.create`.
pub fn create_raw<T>(alloc: &dyn ZigAllocator) -> Result<*mut T, OutOfMemory> {
    let size = std::mem::size_of::<T>();
    let alignment = Alignment::from_byte_units(std::mem::align_of::<T>());
    if size == 0 {
        // if (@sizeOf(T) == 0) return @ptrFromInt(alignment.backward(maxInt(usize)));
        return Ok(zero_len_sentinel(alignment) as *mut T);
    }
    let ptr = alloc.alloc(size, alignment, 0);
    if ptr.is_null() {
        return Err(OutOfMemory);
    }
    Ok(ptr as *mut T)
}

/// PORT NOTE (F-008/W12-01): Zig `alloc.destroy(ptr)` — release the storage of
/// one `T` previously obtained from `create_raw`, WITHOUT dropping the value
/// (Zig has no destructors; run `ptr::drop_in_place` — or move the value out
/// with `ptr::read` — first when the value owns resources). Zero-sized `T` is
/// a no-op, exactly like Zig's `Allocator.destroy`.
///
/// # Safety
/// `ptr` must have been returned by `create_raw::<T>` with the same `alloc`
/// and not yet destroyed; the pointee must not be dropped again afterwards.
pub unsafe fn destroy_raw<T>(alloc: &dyn ZigAllocator, ptr: *mut T) {
    let size = std::mem::size_of::<T>();
    if size == 0 {
        return;
    }
    let alignment = Alignment::from_byte_units(std::mem::align_of::<T>());
    alloc.free(ZigSlice::new(ptr as *mut u8, size), alignment, 0);
}

/// PORT NOTE: The Rust embodiment of a `*const std.mem.Allocator` value — a
/// stable holder for the fat `&dyn ZigAllocator` pointer that the C `Allocator`
/// stores (by thin pointer) in `ctx`. `from_zig` takes `&ZigAllocatorRef`; the
/// ref must remain pointer-stable for the lifetime of the resulting
/// `Allocator`, exactly mirroring the reference's pointer-stability contract.
// PORT NOTE: `Clone + Copy` — the value is morally a `*const
// std.mem.Allocator` (a shared borrow), freely copyable exactly as the Zig
// allocator value is.
#[derive(Clone, Copy)]
pub struct ZigAllocatorRef<'a> {
    inner: &'a dyn ZigAllocator,
}

impl<'a> ZigAllocatorRef<'a> {
    pub fn new(inner: &'a dyn ZigAllocator) -> ZigAllocatorRef<'a> {
        ZigAllocatorRef { inner }
    }
}

/// The VTable required by the C interface.
/// C: RusttyAllocatorVtable
#[repr(C)]
pub struct VTable {
    pub alloc: extern "C" fn(*mut c_void, len: usize, alignment: u8, ret_addr: usize) -> *mut u8,
    pub resize: extern "C" fn(
        *mut c_void,
        memory: *mut u8,
        memory_len: usize,
        alignment: u8,
        new_len: usize,
        ret_addr: usize,
    ) -> bool,
    pub remap: extern "C" fn(
        *mut c_void,
        memory: *mut u8,
        memory_len: usize,
        alignment: u8,
        new_len: usize,
        ret_addr: usize,
    ) -> *mut u8,
    pub free: extern "C" fn(
        *mut c_void,
        memory: *mut u8,
        memory_len: usize,
        alignment: u8,
        ret_addr: usize,
    ),
}

/// Returns an allocator to use for the given possibly-null C allocator,
/// ensuring some allocator is always returned.
///
/// PORT NOTE: The reference selects libc / wasm / SMP allocators based on
/// comptime build flags. In rustty the default backend is the global Rust
/// allocator (`GlobalZigAllocator`), which serves the same role: a single,
/// always-available process allocator. A non-null `c_alloc` is honored exactly
/// as the reference (`c_alloc.zig()`).
pub fn default(c_alloc_: Option<&Allocator>) -> Box<dyn ZigAllocator> {
    // If we're given an allocator, use it.
    if let Some(c_alloc) = c_alloc_ {
        return Box::new(c_alloc.zig());
    }

    // Tests always use the test allocator so we can detect leaks.
    // (rustty: Miri performs leak detection; the global allocator is used.)

    // If we have libc, use that. We prefer libc if we have it because
    // its generally fast but also lets the embedder easily override
    // malloc/free with custom allocators like mimalloc or something.

    // Wasm

    // Freestanding targets don't have a default heap. Using the failing
    // allocator makes a missing allocator show up as out-of-memory.
    //
    // PORT NOTE (`a5423592c`): `builtin.os.tag == .freestanding` is
    // `target_os = "none"`; the wasm arm above (`std.heap.wasm_allocator`) is
    // selected FIRST in the reference, so wasm32-freestanding never reaches
    // this arm — hence the `not(target_arch = "wasm32")` guard. The Rust global
    // allocator is a process-wide fact and has no `#[global_allocator]` on a
    // `none` OS target, so this arm is the only one that can be honest there.
    #[cfg(all(target_os = "none", not(target_arch = "wasm32")))]
    {
        return Box::new(FailingZigAllocator);
    }

    // No libc, use the preferred allocator for releases which is the
    // Zig SMP allocator.
    #[cfg(not(all(target_os = "none", not(target_arch = "wasm32"))))]
    {
        Box::new(GlobalZigAllocator)
    }
}

/// `std.mem.Allocator.failing` — an allocator whose every allocation fails
/// (`alloc`/`remap` return null, `resize` returns false, `free` is a no-op).
/// It is the reference default on native freestanding targets (`a5423592c`),
/// so a caller that forgot to pass an allocator observes plain out-of-memory
/// rather than a missing-heap crash.
#[derive(Clone, Copy, Debug, Default)]
pub struct FailingZigAllocator;

impl ZigAllocator for FailingZigAllocator {
    fn alloc(&self, _len: usize, _alignment: Alignment, _ra: usize) -> *mut u8 {
        std::ptr::null_mut()
    }

    fn resize(
        &self,
        _memory: ZigSlice,
        _alignment: Alignment,
        _new_len: usize,
        _ra: usize,
    ) -> bool {
        false
    }

    fn remap(
        &self,
        _memory: ZigSlice,
        _alignment: Alignment,
        _new_len: usize,
        _ra: usize,
    ) -> *mut u8 {
        std::ptr::null_mut()
    }

    fn free(&self, _memory: ZigSlice, _alignment: Alignment, _ra: usize) {}
}

/// The Allocator interface for custom memory allocation strategies
/// within C librustty APIs.
///
/// This -- purposely -- matches the Zig allocator interface. We do this
/// for two reasons: (1) Zig's allocator interface is well proven in
/// the real world to be flexible and useful, and (2) it allows us to
/// easily convert C allocators to Zig allocators and vice versa, since
/// we're written in Zig.
///
/// C: RusttyAllocator
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Allocator {
    pub ctx: *mut c_void,
    pub vtable: *const VTable,
}

impl Allocator {
    /// Create a C allocator from a Zig allocator. This requires that
    /// the Zig allocator be pointer-stable for the lifetime of the
    /// C allocator.
    ///
    /// PORT NOTE: The reference stores `@ptrCast(@constCast(zig_alloc))` — a
    /// thin pointer to a `*const std.mem.Allocator` (a `{ptr, vtable}` pair) —
    /// in `ctx`. The Rust equivalent of `std.mem.Allocator` is `&dyn
    /// ZigAllocator`, which is itself a fat (two-word) pointer that does not fit
    /// in a single `ctx` word. So `ctx` points at the fat pointer's storage:
    /// the caller passes a `&ZigAllocatorRef` holding that fat pointer (the
    /// moral `*const std.mem.Allocator`), and it must outlive this `Allocator`.
    pub fn from_zig(zig_alloc: &ZigAllocatorRef) -> Allocator {
        Allocator {
            ctx: zig_alloc as *const ZigAllocatorRef as *mut c_void,
            vtable: &ZIG_ALLOCATOR_VTABLE,
        }
    }

    /// Create a Zig allocator from this C allocator. This requires
    /// a pointer to a Zig allocator vtable that we can populate with
    /// our callbacks.
    pub fn zig(&self) -> CAllocatorAsZig {
        CAllocatorAsZig {
            // self.ptr = @ptrCast(@constCast(self))
            ctx: self as *const Allocator as *mut Allocator,
        }
    }
}

/// PORT NOTE: The Zig allocator obtained from `Allocator.zig()`. Its `ptr` is
/// the `Allocator` itself; its vtable (`zig_vtable`) forwards each call to the
/// C `vtable`. This is the Rust embodiment of `std.mem.Allocator{ .ptr = self,
/// .vtable = &zig_vtable }`.
pub struct CAllocatorAsZig {
    ctx: *mut Allocator,
}

impl CAllocatorAsZig {
    /// PORT NOTE: convenience mirroring Zig's `alloc.alloc(u8, n)` — allocate
    /// `n` bytes with byte (1) alignment as a slice. Returns null ptr on
    /// failure (Zig returns `error.OutOfMemory`).
    ///
    /// PORT NOTE (W36-B06): delegates to the trait typed helper so the
    /// zero-length short-circuit (non-null sentinel, no raw callback) applies
    /// here too, exactly like the Zig typed helper it mirrors.
    pub fn alloc_bytes(&self, n: usize) -> *mut u8 {
        ZigAllocator::alloc_bytes(self, n)
    }
}

impl ZigAllocator for CAllocatorAsZig {
    fn alloc(&self, len: usize, alignment: Alignment, ra: usize) -> *mut u8 {
        // const self: *Allocator = @ptrCast(@alignCast(ctx));
        let s: &Allocator = unsafe { &*self.ctx };
        unsafe {
            ((*s.vtable).alloc)(
                s.ctx,
                len,
                alignment.0, // @intFromEnum(alignment)
                ra,
            )
        }
    }

    fn resize(&self, old_mem: ZigSlice, alignment: Alignment, new_len: usize, ra: usize) -> bool {
        let s: &Allocator = unsafe { &*self.ctx };
        unsafe {
            ((*s.vtable).resize)(
                s.ctx,
                old_mem.ptr,
                old_mem.len,
                alignment.0, // @intFromEnum(alignment)
                new_len,
                ra,
            )
        }
    }

    fn remap(&self, old_mem: ZigSlice, alignment: Alignment, new_len: usize, ra: usize) -> *mut u8 {
        let s: &Allocator = unsafe { &*self.ctx };
        unsafe {
            ((*s.vtable).remap)(
                s.ctx,
                old_mem.ptr,
                old_mem.len,
                alignment.0, // @intFromEnum(alignment)
                new_len,
                ra,
            )
        }
    }

    fn free(&self, old_mem: ZigSlice, alignment: Alignment, ra: usize) {
        let s: &Allocator = unsafe { &*self.ctx };
        unsafe {
            ((*s.vtable).free)(
                s.ctx,
                old_mem.ptr,
                old_mem.len,
                alignment.0, // @intFromEnum(alignment)
                ra,
            )
        }
    }
}

/// vtable for the Zig allocator interface to map our extern
/// allocator to Zig's allocator interface.
///
/// PORT NOTE: In the reference this is `Allocator.zig_vtable`, a
/// `std.mem.Allocator.VTable`. In rustty the mapping is provided by the
/// `impl ZigAllocator for CAllocatorAsZig` above (the moral equivalent of the
/// four `zig_vtable` callbacks), so no separate static is required.
///
/// An allocator implementation that wraps a Zig allocator so that
/// it can be exposed to C.
///
/// PORT NOTE: `ctx` is a `*const dyn ZigAllocator` (the Zig allocator passed to
/// `from_zig`). Each callback reconstitutes the trait object and forwards.
static ZIG_ALLOCATOR_VTABLE: VTable = VTable {
    alloc: zig_allocator_alloc,
    resize: zig_allocator_resize,
    remap: zig_allocator_remap,
    free: zig_allocator_free,
};

extern "C" fn zig_allocator_alloc(
    ctx: *mut c_void,
    len: usize,
    alignment: u8,
    ra: usize,
) -> *mut u8 {
    // const zig_alloc: *const std.mem.Allocator = @ptrCast(@alignCast(ctx));
    let zig_alloc: &dyn ZigAllocator = unsafe { (*(ctx as *const ZigAllocatorRef)).inner };
    zig_alloc.alloc(
        len,
        Alignment(alignment), // @enumFromInt(alignment)
        ra,
    )
}

extern "C" fn zig_allocator_resize(
    ctx: *mut c_void,
    memory: *mut u8,
    memory_len: usize,
    alignment: u8,
    new_len: usize,
    ra: usize,
) -> bool {
    let zig_alloc: &dyn ZigAllocator = unsafe { (*(ctx as *const ZigAllocatorRef)).inner };
    let mem = ZigSlice::new(memory, memory_len);
    zig_alloc.resize(
        mem,                  // memory[0..memory_len]
        Alignment(alignment), // @enumFromInt(alignment)
        new_len,
        ra,
    )
}

extern "C" fn zig_allocator_remap(
    ctx: *mut c_void,
    memory: *mut u8,
    memory_len: usize,
    alignment: u8,
    new_len: usize,
    ra: usize,
) -> *mut u8 {
    let zig_alloc: &dyn ZigAllocator = unsafe { (*(ctx as *const ZigAllocatorRef)).inner };
    let mem = ZigSlice::new(memory, memory_len);
    zig_alloc.remap(
        mem,                  // memory[0..memory_len]
        Alignment(alignment), // @enumFromInt(alignment)
        new_len,
        ra,
    )
}

extern "C" fn zig_allocator_free(
    ctx: *mut c_void,
    memory: *mut u8,
    memory_len: usize,
    alignment: u8,
    ra: usize,
) {
    let zig_alloc: &dyn ZigAllocator = unsafe { (*(ctx as *const ZigAllocatorRef)).inner };
    let mem = ZigSlice::new(memory, memory_len);
    zig_alloc.free(
        mem,                  // memory[0..memory_len]
        Alignment(alignment), // @enumFromInt(alignment)
        ra,
    )
}

/// PORT NOTE: The default process allocator backed by Rust's global allocator.
/// This is the rustty analog of `std.heap.c_allocator`/`smp_allocator`: a
/// single, always-available allocator. Each allocation stores nothing extra;
/// callers pass the original `(ptr, len, alignment)` back on free, exactly like
/// the Zig allocator contract (and like libc-via-Zig in the reference).
pub struct GlobalZigAllocator;

impl GlobalZigAllocator {
    /// PORT NOTE (parity S110-F1): this MUST NOT panic. The reference backend is
    /// `std.heap.c_allocator`, which returns `null` for any request it cannot
    /// satisfy; the C API surfaces that as `GHOSTTY_OUT_OF_MEMORY`. Rust's
    /// `Layout::from_size_align` errors when `len` rounded up to `alignment`
    /// exceeds `isize::MAX`, so an `.unwrap()` here turned a C caller asking for
    /// e.g. `SIZE_MAX` bytes into a panic inside an `extern "C"` callback — an
    /// abort across the C ABI where the reference returns null. `None` is the
    /// port's `null`.
    fn layout(len: usize, alignment: Alignment) -> Option<std::alloc::Layout> {
        std::alloc::Layout::from_size_align(len, alignment.to_byte_units()).ok()
    }
}

impl ZigAllocator for GlobalZigAllocator {
    fn alloc(&self, len: usize, alignment: Alignment, _ra: usize) -> *mut u8 {
        if len == 0 {
            // Match Zig: a zero-length alloc returns a valid, aligned,
            // dereference-free pointer.
            // PORT NOTE (W36-B06): the sentinel is Zig 0.16's
            // `alignment.backward(maxInt(usize))` (the HIGHEST aligned
            // address), not the alignment value itself.
            return zero_len_sentinel(alignment);
        }
        // `std.heap.c_allocator` returns null for a request it cannot satisfy;
        // an unrepresentable layout is exactly such a request.
        let Some(layout) = Self::layout(len, alignment) else {
            return core::ptr::null_mut();
        };
        unsafe { std::alloc::alloc(layout) }
    }

    fn resize(&self, memory: ZigSlice, _alignment: Alignment, new_len: usize, _ra: usize) -> bool {
        // In-place resize: only succeed when shrinking-or-equal, matching a
        // conservative allocator that cannot grow in place.
        new_len <= memory.len
    }

    fn remap(&self, memory: ZigSlice, alignment: Alignment, new_len: usize, _ra: usize) -> *mut u8 {
        if memory.len == 0 {
            return self.alloc(new_len, alignment, _ra);
        }
        let Some(old_layout) = Self::layout(memory.len, alignment) else {
            return core::ptr::null_mut();
        };
        // `realloc`'s contract requires the NEW size to be representable too;
        // an unrepresentable growth is a failed remap, i.e. null.
        if Self::layout(new_len, alignment).is_none() {
            return core::ptr::null_mut();
        }
        unsafe { std::alloc::realloc(memory.ptr, old_layout, new_len) }
    }

    fn free(&self, memory: ZigSlice, alignment: Alignment, _ra: usize) {
        if memory.len == 0 {
            return;
        }
        // Unreachable in practice: `alloc`/`remap` return null for any layout
        // this rejects, so no live allocation can carry one. Returning rather
        // than unwrapping keeps the whole vtable panic-free across the C ABI.
        let Some(layout) = Self::layout(memory.len, alignment) else {
            return;
        };
        unsafe { std::alloc::dealloc(memory.ptr, layout) }
    }
}

/// PORT NOTE: A fixed-buffer allocator mirroring `std.heap.FixedBufferAllocator`,
/// used by the "fba allocator" test. Hands out aligned slices from a fixed
/// backing buffer; `free`/`resize` are no-ops except for the last allocation,
/// which is the behavior the test relies upon (alloc then free the tail).
pub struct FixedBufferAllocator {
    buf: *mut u8,
    cap: usize,
    end_index: std::cell::Cell<usize>,
}

impl FixedBufferAllocator {
    pub fn init(buf: &mut [u8]) -> FixedBufferAllocator {
        FixedBufferAllocator {
            buf: buf.as_mut_ptr(),
            cap: buf.len(),
            end_index: std::cell::Cell::new(0),
        }
    }
}

impl ZigAllocator for FixedBufferAllocator {
    fn alloc(&self, len: usize, alignment: Alignment, _ra: usize) -> *mut u8 {
        let align = alignment.to_byte_units();
        let start = self.end_index.get();
        // PORT NOTE: compute the alignment adjustment relative to the buffer
        // base, then derive the result pointer from `self.buf` via `.add` so it
        // keeps provenance from the backing buffer (no usize->ptr round-trip).
        let base = self.buf as usize + start;
        let aligned = (base + align - 1) / align * align;
        let adjust = aligned - (self.buf as usize);
        let new_end = adjust + len;
        if new_end > self.cap {
            return std::ptr::null_mut();
        }
        self.end_index.set(new_end);
        unsafe { self.buf.add(adjust) }
    }

    fn resize(&self, memory: ZigSlice, _alignment: Alignment, new_len: usize, _ra: usize) -> bool {
        let is_last =
            (memory.ptr as usize + memory.len) == (self.buf as usize + self.end_index.get());
        if !is_last {
            return new_len <= memory.len;
        }
        let new_end = (memory.ptr as usize - self.buf as usize) + new_len;
        if new_end > self.cap {
            return false;
        }
        self.end_index.set(new_end);
        true
    }

    fn remap(&self, memory: ZigSlice, alignment: Alignment, new_len: usize, ra: usize) -> *mut u8 {
        if self.resize(memory, alignment, new_len, ra) {
            memory.ptr
        } else {
            std::ptr::null_mut()
        }
    }

    fn free(&self, memory: ZigSlice, _alignment: Alignment, _ra: usize) {
        let is_last =
            (memory.ptr as usize + memory.len) == (self.buf as usize + self.end_index.get());
        if is_last {
            self.end_index.set(self.end_index.get() - memory.len);
        }
    }
}

/// PORT NOTE: A process-static `ZigAllocatorRef` wrapping the global allocator,
/// so `c_allocator()` can return a pointer-stable `Allocator` exactly like the
/// reference's `const c_allocator`. `GlobalZigAllocator` is a zero-sized type
/// with no interior state, so sharing the static across threads is sound — we
/// wrap it to assert `Sync` (the global allocator is itself thread-safe).
static GLOBAL_ZIG_ALLOCATOR: GlobalZigAllocator = GlobalZigAllocator;

struct SyncAllocatorRef(ZigAllocatorRef<'static>);
// SAFETY: the wrapped allocator (GlobalZigAllocator) is a stateless ZST whose
// methods only touch the (thread-safe) global allocator.
unsafe impl Sync for SyncAllocatorRef {}

static C_ALLOCATOR_REF: SyncAllocatorRef = SyncAllocatorRef(ZigAllocatorRef {
    inner: &GLOBAL_ZIG_ALLOCATOR,
});

/// libc Allocator, requires linking libc
///
/// PORT NOTE: `c_allocator: Allocator = .fromZig(&std.heap.c_allocator)`. In
/// rustty the backing allocator is the global Rust allocator held in a process
/// static (`C_ALLOCATOR_REF`), so the returned `Allocator` is pointer-stable
/// for the whole program, matching the reference's `const`. Used by the
/// "c allocator" test.
pub fn c_allocator() -> Allocator {
    Allocator::from_zig(&C_ALLOCATOR_REF.0)
}

// Allocator that can be sent to the C API that does full
// leak checking within Zig tests. This should only be used from
// Zig tests.
//
// PORT NOTE: The reference declares `pub const test_allocator: Allocator =
// .fromZig(&testing.allocator)` (with the doc-comment above, preserved
// verbatim). rustty has no `std.testing.allocator` tracking allocator; under
// R6 the memory gate is enforced by Miri (`cargo +nightly miri test`), which
// performs the leak/UB checking the reference delegated to `testing.allocator`.
// The `test_allocator` const is therefore omitted (no rustty equivalent is
// needed), but its doc-comment is preserved here for R1 fidelity.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_allocator() {
        // PORT NOTE: The reference skips this test without libc
        // (`return error.SkipZigTest`). rustty always has the global allocator
        // backing `c_allocator`, so the test runs. We build the backend on the
        // stack so its pointer stays stable for the Allocator's lifetime.
        let backend = GlobalZigAllocator;
        let backend_ref = ZigAllocatorRef::new(&backend);
        let c_alloc = Allocator::from_zig(&backend_ref);

        let alloc = c_alloc.zig();
        let ptr = alloc.alloc_bytes(10);
        assert!(!ptr.is_null());
        let str_len = 10usize;
        // defer alloc.free(str);
        let mem = ZigSlice::new(ptr, str_len);
        assert_eq!(10, mem.len);
        alloc.free(mem, Alignment(0), 0);
    }

    // PORT NOTE (parity S110-F1 proving test, Rust-only — the reference has no
    // unit test here): the reference backend (`std.heap.c_allocator`) returns
    // NULL for an allocation it cannot satisfy, and the C API turns that into
    // `GHOSTTY_OUT_OF_MEMORY`. The port built the `Layout` with `.unwrap()`, so
    // a request whose size rounded up to the alignment exceeds `isize::MAX`
    // panicked INSIDE an `extern "C"` callback — an abort across the C ABI,
    // where the reference degrades. Exercised through the exported C vtable,
    // which is the path the row cites.
    #[test]
    fn c_vtable_alloc_returns_null_for_an_unrepresentable_request() {
        // (`super::` because this module has a test FUNCTION of the same name.)
        let c_alloc = super::c_allocator();
        let vtable = unsafe { &*c_alloc.vtable };

        // `Layout::from_size_align` rejects anything whose size rounded up to
        // the alignment passes `isize::MAX`; `usize::MAX` does at every
        // alignment, and `isize::MAX` does at any alignment above 1.
        for (len, alignment) in [
            (usize::MAX, 0u8),
            (usize::MAX, 3u8),
            (isize::MAX as usize, 3u8),
        ] {
            let got = (vtable.alloc)(c_alloc.ctx, len, alignment, 0);
            assert!(
                got.is_null(),
                "alloc(len={len}, alignment={alignment}) must be null, not a panic",
            );
        }

        // A representable request still works, so the guard above is not simply
        // rejecting everything.
        let ptr = (vtable.alloc)(c_alloc.ctx, 16, 3, 0);
        assert!(!ptr.is_null());
        (vtable.free)(c_alloc.ctx, ptr, 16, 3, 0);

        // `remap` degrades the same way rather than panicking.
        let ptr = (vtable.alloc)(c_alloc.ctx, 16, 0, 0);
        assert!(!ptr.is_null());
        assert!((vtable.remap)(c_alloc.ctx, ptr, 16, 0, usize::MAX, 0).is_null());
        (vtable.free)(c_alloc.ctx, ptr, 16, 0, 0);
    }

    // PORT NOTE (W36-B06 proving test, Rust-only — the reference has no unit
    // test here): a counting `ZigAllocator` that records every RAW call, so we
    // can assert the typed helpers short-circuit zero lengths WITHOUT touching
    // the raw surface (the Zig `std.mem.Allocator` typed-helper contract).
    struct CountingZigAllocator {
        allocs: std::cell::Cell<usize>,
        frees: std::cell::Cell<usize>,
    }

    impl ZigAllocator for CountingZigAllocator {
        fn alloc(&self, len: usize, alignment: Alignment, ra: usize) -> *mut u8 {
            self.allocs.set(self.allocs.get() + 1);
            GlobalZigAllocator.alloc(len, alignment, ra)
        }
        fn resize(
            &self,
            memory: ZigSlice,
            alignment: Alignment,
            new_len: usize,
            ra: usize,
        ) -> bool {
            GlobalZigAllocator.resize(memory, alignment, new_len, ra)
        }
        fn remap(
            &self,
            memory: ZigSlice,
            alignment: Alignment,
            new_len: usize,
            ra: usize,
        ) -> *mut u8 {
            GlobalZigAllocator.remap(memory, alignment, new_len, ra)
        }
        fn free(&self, memory: ZigSlice, alignment: Alignment, ra: usize) {
            self.frees.set(self.frees.get() + 1);
            GlobalZigAllocator.free(memory, alignment, ra)
        }
    }

    #[test]
    fn typed_helpers_zero_length_short_circuit() {
        let counting = CountingZigAllocator {
            allocs: std::cell::Cell::new(0),
            frees: std::cell::Cell::new(0),
        };
        // Zig `allocator.alloc(u8, 0)`: non-null aligned sentinel, no rawAlloc.
        let ptr = counting.alloc_bytes(0);
        assert!(!ptr.is_null());
        assert_eq!(usize::MAX, ptr as usize); // alignBackward(maxInt(usize), 1)
        assert_eq!(0, counting.allocs.get());
        // Zig `allocator.free(mem[0..0])`: returns before rawFree.
        counting.free_bytes(ZigSlice::new(ptr, 0));
        assert_eq!(0, counting.frees.get());
        // Nonzero paths still hit the raw surface.
        let p = counting.alloc_bytes(16);
        assert!(!p.is_null());
        assert_eq!(1, counting.allocs.get());
        counting.free_bytes(ZigSlice::new(p, 16));
        assert_eq!(1, counting.frees.get());
    }

    #[test]
    fn global_zero_length_sentinel_matches_zig() {
        // Zig 0.16: `alignment.backward(maxInt(usize))`, NOT the alignment value.
        let p1 = GlobalZigAllocator.alloc(0, Alignment(0), 0);
        assert_eq!(usize::MAX, p1 as usize);
        let p8 = GlobalZigAllocator.alloc(0, Alignment(3), 0);
        assert_eq!(usize::MAX & !7, p8 as usize);
        GlobalZigAllocator.free(ZigSlice::new(p1, 0), Alignment(0), 0);
        GlobalZigAllocator.free(ZigSlice::new(p8, 0), Alignment(3), 0);
    }

    // PORT NOTE (F-008/W12-01 proving test, Rust-only): `create_raw` must
    // route through the RAW allocator (caller-observable) and map a rejecting
    // allocator to `OutOfMemory`; `destroy_raw` must return the storage.
    struct RejectingZigAllocator;
    impl ZigAllocator for RejectingZigAllocator {
        fn alloc(&self, _len: usize, _alignment: Alignment, _ra: usize) -> *mut u8 {
            std::ptr::null_mut()
        }
        fn resize(&self, _m: ZigSlice, _a: Alignment, _n: usize, _ra: usize) -> bool {
            false
        }
        fn remap(&self, _m: ZigSlice, _a: Alignment, _n: usize, _ra: usize) -> *mut u8 {
            std::ptr::null_mut()
        }
        fn free(&self, _m: ZigSlice, _a: Alignment, _ra: usize) {}
    }

    #[test]
    fn create_raw_via_caller_allocator() {
        let counting = CountingZigAllocator {
            allocs: std::cell::Cell::new(0),
            frees: std::cell::Cell::new(0),
        };
        // alloc.create(u64) goes through the raw allocator...
        let ptr = create_raw::<u64>(&counting).expect("create must succeed");
        assert_eq!(1, counting.allocs.get());
        unsafe { std::ptr::write(ptr, 42u64) };
        assert_eq!(42, unsafe { *ptr });
        // ...and alloc.destroy(ptr) returns the storage through it.
        unsafe { destroy_raw(&counting, ptr) };
        assert_eq!(1, counting.frees.get());
        // A rejecting allocator is a recoverable error.OutOfMemory.
        assert_eq!(Err(OutOfMemory), create_raw::<u64>(&RejectingZigAllocator));
    }

    #[test]
    fn fba_allocator() {
        let mut buf: [u8; 1024] = [0; 1024];
        let fba = FixedBufferAllocator::init(&mut buf);
        let zig_alloc = fba;

        // Convert the Zig allocator to a C interface
        let zig_alloc_ref = ZigAllocatorRef::new(&zig_alloc);
        let c_alloc: Allocator = Allocator::from_zig(&zig_alloc_ref);

        // Convert back to Zig so we can test it.
        let alloc = c_alloc.zig();
        let ptr = alloc.alloc_bytes(10);
        assert!(!ptr.is_null());
        let str_len = 10usize;
        // defer alloc.free(str);
        let mem = ZigSlice::new(ptr, str_len);
        assert_eq!(10, mem.len);
        alloc.free(mem, Alignment(0), 0);
    }
}
