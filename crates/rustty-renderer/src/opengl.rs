//! PORT STATUS (L4, Linux-only): the OpenGL graphics-API backend — the second
//! concrete graphics API alongside `crate::metal`.
//!
//! PORTS: ghostty `src/renderer/OpenGL.zig` + `src/renderer/opengl/*.zig`, layered
//! over [`gl`] — this crate's hand-port of ghostty's own low-level `pkg/opengl`
//! wrapper package. That two-layer shape is deliberate and mirrors the reference:
//! `pkg/opengl` is a thin hand-rolled wrapper over a RAW glad function-pointer
//! context, so the Rust port keeps a hand-ported `pkg/opengl` twin over the raw
//! `gl` crate (glad's analogue). See the plan's "Tech Stack / GL loader decision".
//!
//! Task 1 lands only [`gl::sys`] — the headless-EGL context plus the
//! `OpenGL.zig::prepareContext` version/debug-output gate. The rest of
//! `OpenGL.zig`, the `pkg/opengl` leaves, the `renderer/opengl/*` leaves and the
//! monomorphic `GenericRenderer(OpenGL)` are later tasks.
//! (docs/superpowers/plans/2026-07-17-L4-opengl-renderer.md)

pub mod gl;

// PORT STATUS (L4 Task 5): the renderer-facing resource leaves, layered over the
// `gl` package (Tasks 2-4). These are the OpenGL twins of `metal/{buffer,texture,
// sampler}.rs`: `Options`-carrying wrappers that drive object creation + upload.
//   * `buffer`  — ports `src/renderer/opengl/buffer.zig`  (the `Buffer(T)` growable store)
//   * `texture` — ports `src/renderer/opengl/Texture.zig` (init + replaceRegion)
//   * `sampler` — ports `src/renderer/opengl/Sampler.zig`
pub mod buffer;
pub mod sampler;
pub mod texture;

// RUSTTY-ONLY smoke tests for the Task-5 leaves — same framing as `metal.rs`'s
// `creates_buffer_texture_sampler_without_panic`: the reference has NO `test{}`
// block over `renderer/opengl/{buffer,Texture,Sampler}.zig`, so there is no parity
// oracle to match. They drive the ported wrappers through a REAL driver's whole
// buffer-grow / texture-upload / sampler-create lifecycle without panicking and
// without provoking a GL error. Skips cleanly (never panics) when the host has no
// OpenGL 4.3 core context — see [`gl::test_support::offscreen_gl`].
#[cfg(test)]
mod tests {
    use super::gl::buffer::{Target, Usage};
    use super::gl::errors::get_error;
    use super::gl::test_support;
    use super::gl::texture as gltex;
    use super::{buffer, sampler, texture};

    /// Mirrors ghostty's `Buffer.sync` grow-path (`renderer/opengl/buffer.zig:76-92`)
    /// and the Metal `buf.sync` grow-path smoke test in `metal.rs`:
    /// `Buffer::<u32>::init(4)`, then `sync(&[6 elems])` must take the
    /// doubled-capacity reallocation path (`self.len = data.len * 2` == 12). Also
    /// exercises `init_fill` (whose `len` is the reference's byte-valued quirk),
    /// the in-capacity `sync` no-realloc path, and `sync_from_array_lists`.
    #[test]
    fn opengl_buffer_grow_and_sync() {
        let Some(_gl) = test_support::offscreen_gl("opengl_buffer_grow_and_sync") else {
            return;
        };
        assert!(get_error().is_ok(), "context started in a GL error state");

        // `Buffer.init` with 4 `u32`s pre-allocated (`buffer.zig:33-47`).
        let mut buf = buffer::Buffer::<u32>::init(
            buffer::Options {
                target: Target::ARRAY,
                usage: Usage::DYNAMIC_DRAW,
            },
            4,
        )
        .expect("Buffer::init");
        assert_eq!(buf.len, 4);
        assert!(get_error().is_ok(), "after Buffer::init");

        // `Buffer.initFill` (`buffer.zig:50-64`). FAITHFULNESS QUIRK: its `len` is
        // `data.len * @sizeOf(T)` (BYTES), unlike `init`'s element count — pinned.
        let filled = buffer::Buffer::<u32>::init_fill(buffer::Options::default(), &[1u32, 2, 3])
            .expect("Buffer::init_fill");
        assert_eq!(filled.len, 3 * std::mem::size_of::<u32>());
        assert!(get_error().is_ok(), "after Buffer::init_fill");

        // THE grow path: 6 elems > 4 -> reallocate to `2 * 6 == 12` (`buffer.zig:81-88`).
        buf.sync(&[10u32, 20, 30, 40, 50, 60])
            .expect("Buffer::sync");
        assert_eq!(buf.len, 12, "grow path sets len to 2*data.len");
        assert!(get_error().is_ok(), "after Buffer::sync grow");

        // In-capacity sync (3 <= 12): must NOT reallocate, `len` untouched.
        buf.sync(&[1u32, 2, 3]).expect("Buffer::sync fit");
        assert_eq!(buf.len, 12, "in-capacity sync leaves len untouched");
        assert!(get_error().is_ok(), "after Buffer::sync fit");

        // `syncFromArrayLists` (`buffer.zig:96-124`) — returns the item total.
        let lists = [vec![7u32, 8], vec![9u32]];
        let n = buf
            .sync_from_array_lists(&lists)
            .expect("Buffer::sync_from_array_lists");
        assert_eq!(n, 3);
        assert!(get_error().is_ok(), "after Buffer::sync_from_array_lists");

        buf.deinit();
        filled.deinit();
        assert!(get_error().is_ok(), "after Buffer::deinit");
    }

    /// Ports the shape of `Texture.init` + `Texture.replaceRegion`
    /// (`renderer/opengl/Texture.zig`): a 2x2 RGBA texture created WITH data (the
    /// `parameter`x4 + `image2D` path), a `replace_region` sub-upload, a second
    /// `init` with `None` data (the `image2D(null)` allocate-only path), and a
    /// `Sampler::init` (the parallel leaf).
    #[test]
    fn opengl_texture_replace_region_smoke() {
        let Some(_gl) = test_support::offscreen_gl("opengl_texture_replace_region_smoke") else {
            return;
        };
        assert!(get_error().is_ok(), "context started in a GL error state");

        // Opaque red / green / blue / white, one texel each.
        #[rustfmt::skip]
        let pixels: [u8; 16] = [
            0xff, 0x00, 0x00, 0xff,   0x00, 0xff, 0x00, 0xff,
            0x00, 0x00, 0xff, 0xff,   0xff, 0xff, 0xff, 0xff,
        ];

        let tex = texture::Texture::init(
            texture::Options {
                format: gltex::Format::RGBA,
                internal_format: gltex::InternalFormat::RGBA,
                target: gltex::Target::Texture2D,
                min_filter: gltex::MinFilter::Linear,
                mag_filter: gltex::MagFilter::Linear,
                wrap_s: gltex::Wrap::ClampToEdge,
                wrap_t: gltex::Wrap::ClampToEdge,
            },
            2,
            2,
            Some(&pixels),
        )
        .expect("Texture::init");
        assert_eq!(tex.width, 2);
        assert_eq!(tex.height, 2);
        assert!(get_error().is_ok(), "after Texture::init");

        // `replaceRegion` — overwrite the (1,1) texel (`Texture.zig:84-104`).
        tex.replace_region(1, 1, 1, 1, &[0x12, 0x34, 0x56, 0x78])
            .expect("Texture::replace_region");
        assert!(get_error().is_ok(), "after Texture::replace_region");

        // `init` with `null` data — the allocate-only `image2D(…, null)` path.
        let empty = texture::Texture::init(
            texture::Options {
                format: gltex::Format::RGBA,
                internal_format: gltex::InternalFormat::RGBA,
                target: gltex::Target::Texture2D,
                min_filter: gltex::MinFilter::Nearest,
                mag_filter: gltex::MagFilter::Nearest,
                wrap_s: gltex::Wrap::ClampToEdge,
                wrap_t: gltex::Wrap::ClampToEdge,
            },
            4,
            4,
            None,
        )
        .expect("Texture::init(None)");
        assert!(get_error().is_ok(), "after Texture::init(None)");

        // `Sampler::init` (`renderer/opengl/Sampler.zig:28-41`).
        let smp = sampler::Sampler::init(sampler::Options {
            min_filter: gltex::MinFilter::Linear,
            mag_filter: gltex::MagFilter::Linear,
            wrap_s: gltex::Wrap::ClampToEdge,
            wrap_t: gltex::Wrap::ClampToEdge,
        })
        .expect("Sampler::init");
        assert!(get_error().is_ok(), "after Sampler::init");

        smp.deinit();
        empty.deinit();
        tex.deinit();
        assert!(get_error().is_ok(), "after deinit");
    }
}
