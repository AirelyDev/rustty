//! This contains convenience functions for allocating various types.
//!
//! The primary use case for this is Wasm builds. rustty relies a lot on
//! pointers to various types for ABI compatibility and creating those pointers
//! in Wasm is tedious. This file contains a purely additive set of functions
//! that can be exposed to the Wasm module without changing the API from the
//! C library.
//!
//! Given these are convenience methods, they always use the default allocator.
//! If a caller is using a custom allocator, they have the expertise to
//! allocate these types manually using their custom allocator.

// Get our default allocator at comptime since it is known.
//
// PORT NOTE: The reference binds `alloc` at comptime via `default(null)`. In
// rustty the default allocator is the global Rust allocator; each convenience
// function uses it directly (`std::alloc`) with the matching `Layout`, which is
// the same backend `default(None)` returns.
use std::alloc::{alloc as raw_alloc, dealloc, Layout};
use std::ffi::c_void;

pub type Opaque = *mut c_void;

/// PORT NOTE: `alloc.create(*anyopaque)` allocates storage for one pointer.
pub extern "C" fn alloc_opaque() -> *mut Opaque {
    let layout = Layout::new::<Opaque>();
    let p = unsafe { raw_alloc(layout) } as *mut Opaque;
    // catch return null
    p
}

pub extern "C" fn free_opaque(ptr: *mut Opaque) {
    if !ptr.is_null() {
        let layout = Layout::new::<Opaque>();
        unsafe { dealloc(ptr as *mut u8, layout) };
    }
}

pub extern "C" fn alloc_u8_array(len: usize) -> *mut u8 {
    let slice = match alloc_array::<u8>(len) {
        Some(p) => p,
        None => return std::ptr::null_mut(),
    };
    slice
}

pub extern "C" fn free_u8_array(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        free_array::<u8>(ptr, len);
    }
}

pub extern "C" fn alloc_u16_array(len: usize) -> *mut u16 {
    let slice = match alloc_array::<u16>(len) {
        Some(p) => p,
        None => return std::ptr::null_mut(),
    };
    slice
}

pub extern "C" fn free_u16_array(ptr: *mut u16, len: usize) {
    if !ptr.is_null() {
        free_array::<u16>(ptr, len);
    }
}

pub extern "C" fn alloc_u8() -> *mut u8 {
    let layout = Layout::new::<u8>();
    unsafe { raw_alloc(layout) }
    // catch return null
}

pub extern "C" fn free_u8(ptr: *mut u8) {
    if !ptr.is_null() {
        let layout = Layout::new::<u8>();
        unsafe { dealloc(ptr, layout) };
    }
}

pub extern "C" fn alloc_usize() -> *mut usize {
    let layout = Layout::new::<usize>();
    unsafe { raw_alloc(layout) as *mut usize }
    // catch return null
}

pub extern "C" fn free_usize(ptr: *mut usize) {
    if !ptr.is_null() {
        let layout = Layout::new::<usize>();
        unsafe { dealloc(ptr as *mut u8, layout) };
    }
}

// PORT NOTE: shared helpers for the `[*]T` array allocations above. They model
// `alloc.alloc(T, len)` / `alloc.free(p[0..len])` with the matching `Layout`.
fn alloc_array<T>(len: usize) -> Option<*mut T> {
    let layout = Layout::array::<T>(len).ok()?;
    if layout.size() == 0 {
        return Some(layout.align() as *mut T);
    }
    let p = unsafe { raw_alloc(layout) } as *mut T;
    if p.is_null() {
        None
    } else {
        Some(p)
    }
}

fn free_array<T>(ptr: *mut T, len: usize) {
    let layout = Layout::array::<T>(len).unwrap();
    if layout.size() == 0 {
        return;
    }
    unsafe { dealloc(ptr as *mut u8, layout) };
}
