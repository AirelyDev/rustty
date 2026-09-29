//! The cross-thread pieces of `generic.zig`'s `Renderer(GraphicsAPI)` that the
//! apprt's main thread reaches while the render thread runs: the `LatestFrame`
//! single-slot queue (`generic.zig:992-1030` at `12752b2ac`, `2f0b65346`) and the
//! handle to the renderer's `draw_mutex`.
//!
//! Both `generic` twins (`generic.rs` for Metal, `generic/linux_impl.rs` for
//! OpenGL) share this module, so neither twin can drift from the other here.
//!
//! PORT NOTE (N437, aliasing model): both pieces live in their OWN heap
//! allocation, reached through an `Arc` handle that each thread takes before the
//! render thread starts. A thread that waits on the draw mutex, or that takes the
//! latest frame, then touches no byte of the `Renderer` itself, so the thread
//! that holds the lock may form a whole-`Renderer` reference for the locked
//! region without a foreign access popping it. Zig's `*Renderer` has no aliasing
//! rule, so the reference keeps both inline; under Stacked and Tree Borrows an
//! inline mutex that another thread waits on is a foreign access to the bytes a
//! whole-`Renderer` `&mut` covers. Proven on the isolated harness
//! `.port/phase-2/mirihar-n437/` (the inline-mutex shape is UB, this shape is
//! clean, SB and TB).

use std::sync::Arc;

use rustty_zigstd::thread::Mutex;

/// What `generic.zig` spells `ExportedFrame == void`: whether the graphics API
/// exports frames for the apprt to composite at all.
//
// PORT NOTE: `pub const ExportedFrame = if (@hasDecl(GraphicsAPI,
// "ExportedFrame")) GraphicsAPI.ExportedFrame else void;` is a comptime type
// choice; the monomorphic port models the `void` arm as `()` and the
// `comptime ExportedFrame == void` tests as this associated const, so the
// `LatestFrame` methods fold to the reference's early returns for Metal.
pub trait ExportedFrameType {
    /// True for the reference's `void` (Metal): pushes and takes are no-ops.
    const IS_VOID: bool;
}

impl ExportedFrameType for () {
    const IS_VOID: bool = true;
}

/// A thread-safe, single-slot "latest wins" queue. The render thread
/// calls `push` with the latest frame; the apprt calls `take` in its
/// snapshot handler to grab the most recent frame. Old frames are
/// dropped and released. For a terminal this is correct — we never
/// want to queue up frames behind a slow compositor.
//
// PORT NOTE: `mutex: std.Io.Mutex = .init, latest: ?ExportedFrame = null`
// becomes one `std::sync::Mutex<Option<F>>`: the reference only ever touches
// `latest` under `mutex`, which is exactly what the std mutex's guard enforces.
// `old.deinit()` / `v.deinit()` are the frame's `Drop` (an OpenGL frame owns its
// DMA-BUF fds or its pixel buffer). Poisoning is ignored: the slot is valid in
// every state (Zig has no poisoning).
pub struct LatestFrame<F: ExportedFrameType> {
    latest: std::sync::Mutex<Option<F>>,
}

impl<F: ExportedFrameType> Default for LatestFrame<F> {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: ExportedFrameType> LatestFrame<F> {
    /// `latest_frame: LatestFrame = .{}`.
    pub const fn new() -> Self {
        LatestFrame {
            latest: std::sync::Mutex::new(None),
        }
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Option<F>> {
        match self.latest.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// PORTS: `LatestFrame.push`.
    ///
    /// ```zig
    /// pub fn push(self: *LatestFrame, io: std.Io, value: ExportedFrame) void {
    ///     if (comptime ExportedFrame == void) return;
    ///
    ///     self.mutex.lockUncancelable(io);
    ///     defer self.mutex.unlock(io);
    ///     if (self.latest) |*old| old.deinit();
    ///     self.latest = value;
    /// }
    /// ```
    pub fn push(&self, value: F) {
        if F::IS_VOID {
            return;
        }

        let mut latest = self.slot();
        // if (self.latest) |*old| old.deinit(); self.latest = value;
        // (The replaced frame drops when `old` goes out of scope, under the lock
        // as in the reference.)
        let old = latest.replace(value);
        drop(old);
    }

    /// PORTS: `LatestFrame.take`.
    ///
    /// ```zig
    /// pub fn take(self: *LatestFrame, io: std.Io) ?ExportedFrame {
    ///     if (comptime ExportedFrame == void) return null;
    ///
    ///     self.mutex.lockUncancelable(io);
    ///     defer self.mutex.unlock(io);
    ///     const result = self.latest orelse return null;
    ///     self.latest = null;
    ///     return result;
    /// }
    /// ```
    pub fn take(&self) -> Option<F> {
        if F::IS_VOID {
            return None;
        }

        self.slot().take()
    }

    /// PORTS: `LatestFrame.deinit`.
    ///
    /// ```zig
    /// pub fn deinit(self: *LatestFrame, io: std.Io) void {
    ///     if (comptime ExportedFrame == void) return;
    ///
    ///     self.mutex.lockUncancelable(io);
    ///     defer self.mutex.unlock(io);
    ///     if (self.latest) |*v| v.deinit();
    ///     self.latest = null;
    /// }
    /// ```
    pub fn deinit(&self) {
        if F::IS_VOID {
            return;
        }

        let mut latest = self.slot();
        let v = latest.take();
        drop(v);
    }
}

/// The renderer's `draw_mutex`, in its own allocation.
///
/// This mutex must be held whenever any state used in `drawFrame` is being
/// modified, and also when it's being accessed in `drawFrame`.
//
// PORT NOTE (N437): see the module note. `Clone` hands out another handle to
// the SAME mutex; core takes one for the render thread (`renderer_seam::Thread`)
// and one for the main thread (`renderer_seam::Renderer`) before the render
// thread starts, and every cross-thread entry locks through its own handle
// before it forms any reference to the renderer. `Deref` keeps the renderer's
// own single-threaded paths (`self.draw_mutex.lock()`) unchanged.
#[derive(Clone)]
pub struct DrawMutex(Arc<Mutex>);

impl DrawMutex {
    /// Zig `draw_mutex: std.Io.Mutex = .init` — a new, unlocked mutex.
    pub fn new() -> DrawMutex {
        DrawMutex(Arc::new(Mutex::new()))
    }

    /// Whether two handles name the same mutex.
    pub fn same(&self, other: &DrawMutex) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Default for DrawMutex {
    fn default() -> Self {
        Self::new()
    }
}

impl core::ops::Deref for DrawMutex {
    type Target = Mutex;

    fn deref(&self) -> &Mutex {
        &self.0
    }
}

impl core::fmt::Debug for DrawMutex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("DrawMutex")
    }
}

#[cfg(test)]
mod tests {
    use super::{DrawMutex, ExportedFrameType, LatestFrame};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A frame that counts its own drops (the reference's `deinit`).
    struct Frame {
        id: usize,
        drops: Arc<AtomicUsize>,
    }

    impl ExportedFrameType for Frame {
        const IS_VOID: bool = false;
    }

    impl Drop for Frame {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn frame(id: usize, drops: &Arc<AtomicUsize>) -> Frame {
        Frame {
            id,
            drops: drops.clone(),
        }
    }

    #[test]
    fn latest_frame_newest_wins_and_releases_the_replaced_frame() {
        let drops = Arc::new(AtomicUsize::new(0));
        let slot: LatestFrame<Frame> = LatestFrame::new();
        assert!(slot.take().is_none());

        slot.push(frame(1, &drops));
        slot.push(frame(2, &drops));
        // The replaced frame was released by the push.
        assert_eq!(drops.load(Ordering::SeqCst), 1);

        let taken = slot.take().expect("a frame");
        assert_eq!(taken.id, 2);
        // `take` empties the slot and hands ownership to the caller.
        assert!(slot.take().is_none());
        drop(taken);
        assert_eq!(drops.load(Ordering::SeqCst), 2);

        slot.push(frame(3, &drops));
        slot.deinit();
        assert_eq!(drops.load(Ordering::SeqCst), 3);
        assert!(slot.take().is_none());
    }

    #[test]
    fn latest_frame_is_a_no_op_for_a_void_frame() {
        let slot: LatestFrame<()> = LatestFrame::new();
        slot.push(());
        assert!(slot.take().is_none());
        slot.deinit();
    }

    /// Miri target (P7-W7 T2): a render thread pushing while the main thread
    /// takes. Every frame is released exactly once, whichever side releases it,
    /// and the main thread only ever sees frames in push order.
    #[test]
    fn latest_frame_concurrent_push_and_take_release_every_frame_once() {
        const N: usize = if cfg!(miri) { 40 } else { 2000 };
        let drops = Arc::new(AtomicUsize::new(0));
        let slot: Arc<LatestFrame<Frame>> = Arc::new(LatestFrame::new());

        let producer = {
            let slot = slot.clone();
            let drops = drops.clone();
            std::thread::spawn(move || {
                for id in 1..=N {
                    slot.push(frame(id, &drops));
                }
            })
        };

        let mut last = 0;
        let mut taken = 0;
        while !producer.is_finished() || taken == 0 {
            if let Some(f) = slot.take() {
                assert!(f.id > last, "frames arrive in push order");
                last = f.id;
                taken += 1;
            }
            std::thread::yield_now();
        }
        producer.join().unwrap();
        if let Some(f) = slot.take() {
            assert!(f.id > last);
        }
        slot.deinit();

        assert_eq!(drops.load(Ordering::SeqCst), N, "every frame released once");
    }

    #[test]
    fn draw_mutex_handles_share_one_mutex() {
        let a = DrawMutex::new();
        let b = a.clone();
        assert!(a.same(&b));
        assert!(!a.same(&DrawMutex::new()));
        a.lock();
        b.unlock();
    }
}
