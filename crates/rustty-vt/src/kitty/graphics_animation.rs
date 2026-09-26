//! Kitty graphics protocol animation support.
//!
//! https://sw.kovidgoyal.net/kitty/graphics-protocol/#animation
//!
//! An animation is a set of frames attached to an existing image.
//! Frame 1 (the "root frame") is the image's own base data. Frames
//! 2..N are stored here. Unlike Kitty, which stores frames as delta
//! rectangles in a disk cache and composes them lazily, we compose
//! every frame eagerly into a full image-sized RGBA buffer at load
//! time. This trades some memory for a much simpler model: a frame is
//! always ready to display and there are no reference chains to
//! maintain, coalesce, or garbage collect. Frame storage is bounded
//! by the same byte limit as image storage.
//!
//! To keep composition to a single pixel format, an image is
//! converted to RGBA the first time an animation command composes
//! into it, and all transmitted frame data is converted to RGBA
//! before composition. Blending an opaque (alpha=255) source pixel
//! degenerates to a copy, so this loses no information relative to
//! Kitty's separate RGB/RGBA paths. The pixel-level primitives
//! (conversion, fill, rectangle composition) live in
//! graphics_pixel.rs.
//!
//! rustty-vt kitty/graphics_animation
//!
//! PORTS: ghostty src/terminal/kitty/graphics_animation.zig
//!
//! PORT STATUS: ported

// const std = @import("std");
// const Allocator = std.mem.Allocator;
use crate::alloc::{AVec, Allocator, Bytes};

/// The gap assigned to a newly created frame when the command doesn't
/// specify one (z omitted or z=0). Taken from Kitty (DEFAULT_GAP).
pub const DEFAULT_GAP_MS: u32 = 40;

/// Animation state for a single image. Heap-allocated and owned by
/// the Image. This is created lazily by the first animation command that
/// needs it.
//
// PORT NOTE: the reference `Image.animation` is `?*Animation` — a raw heap
// pointer that Image value-copies SHARE and only the stored image's explicit
// `deinit` frees. Rustty's stored `Image` owns it as an allocator-aware box
// (`ABox<Animation>`, allocated through the storage's allocator): the stored
// image owns it and `Drop` frees it at every removal site
// (replace/evict/delete/clear); copies made for reading
// (`Image::withoutData`) do not carry it. `Clone`/`PartialEq`/`Eq` derives
// are Rust-only glue required by `Image`'s derives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    /// The frames following the root frame: `frames[i]` is
    /// protocol frame number `i + 2`. The root frame (frame 1) is the
    /// image's base data and is not stored here.
    // PORT NOTE (parity tail S7-4): `std.ArrayListUnmanaged(Frame) = .empty`,
    // grown (and each frame's `data` allocated) through the storage's
    // allocator; bound when the animation is created (`init_in`).
    pub frames: AVec<Frame>,

    /// The gap of the root frame in milliseconds. Zero means gapless:
    /// the frame is skipped during playback. Kitty creates the root
    /// frame with a zero gap; the client gives it one with a=a,r=1,z=N.
    pub root_gap_ms: u32,

    /// The zero-based index of the frame currently displayed: 0 is
    /// the root frame, i >= 1 is frames[i - 1].
    pub current_index: u32,

    /// Playback state. Every image starts stopped (client-driven).
    pub state: State,

    /// Maximum number of loops to play (0 = infinite). Set from the
    /// a=a v key as v-1, per the protocol's off-by-one encoding.
    pub max_loops: u32,

    /// Number of completed loops since playback started.
    pub current_loop: u32,

    /// Timestamp in milliseconds (on the ticker's clock, see
    /// ImageStorage.animationTick) when the current frame was shown.
    /// Null means "not yet shown"; the next tick stamps it.
    pub frame_shown_at_ms: Option<u64>,
}

// pub const State = enum { stopped, loading, running };
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    /// Not advancing; frames change only via a=a c=N (client-driven).
    #[default]
    Stopped,
    /// Advancing, but playback parks on the last frame waiting
    /// for more frames instead of looping (a=a s=2).
    Loading,
    /// Advancing and looping (a=a s=3).
    Running,
}

// pub const Frame = struct { data: []u8, gap_ms: u32 };
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Fully composed pixel data, always image width * height * 4
    /// bytes of RGBA.
    pub data: Bytes,

    /// Milliseconds this frame is displayed before advancing.
    /// Zero means gapless: skipped during playback.
    pub gap_ms: u32,
}

/// `.{}` over the Rust heap (the empty list allocates nothing).
impl Default for Animation {
    fn default() -> Animation {
        Animation::init_in(crate::alloc::Global)
    }
}

impl Animation {
    /// `.{}` whose `frames` grow through `alloc` (the reference passes that
    /// allocator to every `frames.append`/`deinit`).
    pub fn init_in(alloc: Allocator) -> Animation {
        Animation {
            frames: AVec::new_in(alloc),
            root_gap_ms: 0,
            current_index: 0,
            state: State::Stopped,
            max_loops: 0,
            current_loop: 0,
            frame_shown_at_ms: None,
        }
    }

    // pub fn deinit(self: *Animation, alloc: Allocator) void {
    //     for (self.frames.items) |frame| alloc.free(frame.data);
    //     self.frames.deinit(alloc);
    // }
    // PORT NOTE: dropping `frames` frees every frame's data and the list
    // through the allocator they were allocated with; `deinit` checks it.
    pub fn deinit(&mut self, alloc: Allocator) {
        if self.frames.capacity() != 0 {
            self.frames.allocator().debug_assert_same(alloc);
        }
        self.frames = AVec::new_in(alloc);
    }

    /// The total number of frames including the root frame.
    pub fn frame_count(&self) -> u32 {
        // return @intCast(self.frames.items.len + 1);
        u32::try_from(self.frames.len() + 1).unwrap()
    }

    /// The gap of the frame at the given zero-based index.
    pub fn gap_at(&self, index: u32) -> u32 {
        if index == 0 {
            return self.root_gap_ms;
        }
        self.frames[(index - 1) as usize].gap_ms
    }

    /// Set the gap of the frame at the given zero-based index.
    pub fn set_gap_at(&mut self, index: u32, gap_ms: u32) {
        if index == 0 {
            self.root_gap_ms = gap_ms;
        } else {
            self.frames[(index - 1) as usize].gap_ms = gap_ms;
        }
    }

    /// The sum of all frame gaps. An animation with a zero duration
    /// (every frame gapless) never advances; this doubles as the
    /// guard that keeps the gapless-skip loop in animationTick from
    /// spinning forever, exactly like Kitty's animation_duration.
    pub fn duration_ms(&self) -> u64 {
        let mut total: u64 = self.root_gap_ms as u64;
        for frame in &self.frames {
            total += frame.gap_ms as u64;
        }
        total
    }

    /// Bytes of frame data held by this animation, counted against
    /// the image storage limit.
    pub fn frame_bytes(&self) -> usize {
        let mut total: usize = 0;
        for frame in &self.frames {
            total += frame.data.len();
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "animation gap helpers"
    #[test]
    fn animation_gap_helpers() {
        let mut anim: Animation = Animation::default();
        // defer anim.deinit(alloc); — Drop.
        // try anim.frames.append(alloc, .{ .data = try alloc.alloc(u8, 4), .gap_ms = 100 });
        let alloc = crate::alloc::Global;
        anim.frames.push(Frame {
            data: crate::alloc::try_dupe(alloc, &[0u8; 4]).unwrap(),
            gap_ms: 100,
        });

        assert_eq!(2u32, anim.frame_count());
        assert_eq!(0u32, anim.gap_at(0));
        assert_eq!(100u32, anim.gap_at(1));
        assert_eq!(100u64, anim.duration_ms());
        assert_eq!(4usize, anim.frame_bytes());

        anim.set_gap_at(0, 40);
        anim.set_gap_at(1, 60);
        assert_eq!(100u64, anim.duration_ms());
        assert_eq!(40u32, anim.root_gap_ms);
    }
}
