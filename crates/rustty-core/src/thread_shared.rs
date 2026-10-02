//! PORT-ONLY (N444, P7-W7 T5c): state inside the core `Surface` that another
//! thread writes, each block in its own allocation.
//!
//! The reference embeds these blocks by value in `Surface` and hands their
//! addresses to the thread that writes them:
//!
//! - `io: termio.Termio` (the io read thread writes `io.terminal` under
//!   `renderer_state.mutex`; the io writer thread drains `io.mailbox`);
//! - `io_thread: termio.Thread` and `renderer_thread: rendererpkg.Thread` (their
//!   own threads' loop state);
//! - `renderer_state: rendererpkg.State` (the D4 handoff atomics the io and
//!   render threads bump);
//! - `search.state: terminal.search.Thread` (the search thread's loop state).
//!
//! Zig's `*Surface` carries no aliasing rule. In Rust, every main-thread
//! `&mut Surface` / `&Surface` the apprt forms (`sizeCallback`,
//! `contentScaleCallback`, key, mouse, focus, clipboard, ...) is a retag over
//! every byte of the surface, which races with those writes (Stacked and Tree
//! Borrows; `.port/phase-2/mirihar-n444`). Out of line, a surface retag covers
//! the pointer only. This is the renderer's fix (N441,
//! [`crate::renderer_seam::RendererBox`]) applied to the other blocks.

use std::mem::MaybeUninit;

/// One cross-thread block of the core `Surface`, in its own allocation. The
/// other thread and the surface share the one raw root from `Box::into_raw`;
/// the main thread reaches the block by place through [`as_ptr`], never by a
/// whole-block reference while the other thread runs.
///
/// There is no `Drop`: every owner of a core `Surface` frees it without drop
/// glue (embedded `closeSurface` through `ManuallyDrop`, GTK through
/// `dealloc`, the init-error callers through the `MaybeUninit` slot), so each
/// teardown site frees its block explicitly. It tears the block down as it
/// did inline (`deinit`, or `ptr::read(..).deinit()` for a consuming one),
/// then either
/// - calls [`drop_and_dealloc`] when the block is initialized and its
///   remaining fields own Rust heap that no `deinit` releases (N502: the
///   render thread's block, whose loop box and `DrawMutex` handle leaked with
///   every closed surface, and the renderer-state block, whose std
///   `Mutex`/`Condvar` box their pthread objects on Darwin), or
/// - calls [`dealloc`], without drop glue, when the block was never
///   initialized (an init-error slot) or was read out and dropped by value
///   already (`ptr::read(..).deinit()`, whose copy drops itself).
///
/// [`as_ptr`]: ThreadSharedBox::as_ptr
/// [`dealloc`]: ThreadSharedBox::dealloc
/// [`drop_and_dealloc`]: ThreadSharedBox::drop_and_dealloc
pub struct ThreadSharedBox<T>(*mut T);

impl<T> ThreadSharedBox<T> {
    /// Move an initialized block into its own allocation.
    pub fn new(value: T) -> ThreadSharedBox<T> {
        ThreadSharedBox(Box::into_raw(Box::new(value)))
    }

    /// Allocate an UNINITIALIZED block. `Surface.init` hands a block's address
    /// out before the block exists (`&self.renderer_thread` to the renderer,
    /// `&self.renderer_state` to the render thread, `&self.io.terminal` to the
    /// render state), then writes it in place through [`as_ptr`].
    ///
    /// [`as_ptr`]: ThreadSharedBox::as_ptr
    pub fn new_uninit() -> ThreadSharedBox<T> {
        ThreadSharedBox(Box::into_raw(Box::<T>::new_uninit()).cast::<T>())
    }

    /// The raw root the other thread and the surface share. Never null.
    #[inline]
    pub fn as_ptr(&self) -> *mut T {
        self.0
    }

    /// Free the allocation WITHOUT running `T`'s drop glue.
    ///
    /// # Safety
    /// The block's own teardown has run (or the block was never initialized),
    /// the thread that wrote it is joined or was never spawned, nothing uses
    /// [`ThreadSharedBox::as_ptr`] afterwards, and this is called once.
    pub unsafe fn dealloc(&self) {
        // SAFETY: `self.0` came from `Box::into_raw` of a `Box<T>` or a
        // `Box<MaybeUninit<T>>` (same layout), and the contract above makes
        // this the last use. `MaybeUninit` has no drop glue.
        unsafe { drop(Box::from_raw(self.0.cast::<MaybeUninit<T>>())) };
    }

    /// Drop the block NORMALLY and free the allocation, after its explicit
    /// `deinit`: for a block whose fields still own Rust heap that `deinit`
    /// leaves (N502: the core `Surface`'s render-thread and renderer-state
    /// blocks, in `Surface::deinit` and `initErrdefers`), and for one whose
    /// inline owner was dropped normally (the search state: `Search` values are
    /// dropped normally, unlike the surface). No field's `Drop` may repeat a
    /// teardown `deinit` already did.
    ///
    /// # Safety
    /// The block is initialized and its explicit teardown has run, leaving it
    /// safe to drop; the thread that wrote it is joined or was never spawned;
    /// nothing uses [`ThreadSharedBox::as_ptr`] afterwards, and this is called
    /// once.
    pub unsafe fn drop_and_dealloc(&self) {
        // SAFETY: `self.0` came from `Box::into_raw(Box::new(..))`, the block
        // is initialized, and the contract above makes this the last use.
        unsafe { drop(Box::from_raw(self.0)) };
    }
}
