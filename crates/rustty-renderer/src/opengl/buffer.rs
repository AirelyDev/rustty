//! PORTS: ghostty `src/renderer/opengl/buffer.zig` (126 loc) — the growable
//! `Buffer(comptime T)` GPU data store, layered over the `gl` package's
//! [`gl::Buffer`](super::gl::Buffer) leaf (Task 2). This is the OpenGL twin of
//! `metal/buffer.rs`.
//!
//! PORT NOTE (`Buffer(comptime T)` -> `Buffer<T>`): as in the Metal twin, the Zig
//! comptime `Buffer(T)` maps 1:1 onto a concrete Rust generic — `T` appears only
//! in `@sizeOf(T)`, so it is carried by a [`PhantomData<T>`] rather than any field.
//! Unlike the Metal twin, NO `T: Copy` bound is needed: the underlying
//! [`gl::buffer::Binding`](super::gl::buffer) uploads `T`'s bytes through a raw
//! pointer (`set_data`/`set_sub_data`), it never copies a `T` value.
//!
//! PORT NOTE (`gl.Buffer` field, no ARC): where the Metal twin holds a
//! `Retained<…>` and dissolves `deinit`/reallocation into `Drop`, the `gl` leaf
//! [`gl::Buffer`](super::gl::Buffer) is a `Copy` `GLuint` handle with EXPLICIT
//! `create`/`destroy` (the reference manages GL lifetimes by hand). So `deinit`
//! is a real by-value method here (not a `Drop` impl), exactly as
//! `buffer.zig:66-68` has it, and the grow path destroys nothing — GL's
//! `glBufferData` re-specifies the SAME buffer name's store in place
//! (`buffer.zig:84-88`), it does not allocate a new name. This wrapper is
//! therefore deliberately NOT `Copy` (a copy + double `deinit` would double-delete
//! the shared handle), and NOT `Drop` (the reference leaks on a forgotten
//! `deinit`, and reproducing that is faithful; an RAII `Drop` would be a deviation).

use std::marker::PhantomData;

use super::gl::buffer::{Buffer as GlBuffer, Target, Usage};
use super::gl::errors;

/// Options for initializing a buffer.
//
// PORTS: `buffer.zig:9-13`.
//
// ```zig
// pub const Options = struct {
//     target: gl.Buffer.Target = .array,
//     usage: gl.Buffer.Usage = .dynamic_draw,
// };
// ```
//
// PORT NOTE (per-file `Options`, NOT flattened to `BufferOptions`): unlike the
// Metal twin — which had to rename its three leaf option structs because they
// share the `metal::` scope — the OpenGL leaves live in distinct modules
// (`opengl::buffer`/`opengl::texture`/`opengl::sampler`), so each keeps the
// reference's own `Options` spelling, restoring Zig's per-file `buffer.Options`
// namespacing exactly (the brief names `opengl::buffer::Options`).
//
// PORT NOTE (Zig field defaults -> `Default`): Zig's `= .array` / `= .dynamic_draw`
// field defaults have no per-field Rust equivalent, so they are collected into a
// `Default` impl. A `.{}` in the reference becomes `Options::default()`; a partial
// override becomes `Options { target: …, ..Default::default() }`.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub target: Target,
    pub usage: Usage,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            target: Target::ARRAY,
            usage: Usage::DYNAMIC_DRAW,
        }
    }
}

/// OpenGL data storage for a certain set of equal types. This is usually
/// used for vertex buffers, etc. This helpful wrapper makes it easy to
/// prealloc, shrink, grow, sync, buffers with OpenGL.
//
// PORTS: `buffer.zig:18-30` — `buffer: gl.Buffer, opts: Options, len: usize`.
pub struct Buffer<T> {
    /// Underlying `gl.Buffer` instance.
    pub buffer: GlBuffer,

    /// Options this buffer was allocated with.
    pub opts: Options,

    /// Current allocated length of the data store.
    /// Note this is the number of `T`s, not the size in bytes.
    //
    // FAITHFULNESS QUIRK (R1): `init` sets this to the ELEMENT count while
    // `init_fill` sets it to the BYTE count (`data.len * @sizeOf(T)`) — a
    // reference inconsistency (`buffer.zig:46` vs `:62`), reproduced verbatim
    // below rather than harmonized. The doc comment above is the reference's own,
    // and it too is only true for the `init` path.
    // RE-AFFIRMED (P3-W0 T5): checked against oracle range 4749c4e93..6ad1fe7d8 — src/renderer/opengl/buffer.zig is UNTOUCHED there, so this quirk is still faithful. Ledger: .port/tools/gates/stale-comments-D9.tsv.
    pub len: usize,

    // PORT NOTE: `T` is carried in the Zig type `Buffer(T)` itself; Rust needs a
    // `PhantomData<T>` because `T` names no field.
    _marker: PhantomData<T>,
}

impl<T> Buffer<T> {
    /// Initialize a buffer with the given length pre-allocated.
    //
    // PORTS: `buffer.zig:33-47`.
    //
    // ```zig
    // pub fn init(opts: Options, len: usize) !Self {
    //     const buffer = try gl.Buffer.create();
    //     errdefer buffer.destroy();
    //     const binding = try buffer.bind(opts.target);
    //     defer binding.unbind();
    //     try binding.setDataNullManual(len * @sizeOf(T), opts.usage);
    //     return .{ .buffer = buffer, .opts = opts, .len = len };
    // }
    // ```
    //
    // PORT NOTE (`errdefer buffer.destroy()` + `defer binding.unbind()`): Zig runs
    // deferred cleanups in reverse declaration order at scope exit, `errdefer` only
    // on an error return. So on a `setDataNullManual` failure the ORDER is `unbind`
    // (defer) then `destroy` (errdefer); the explicit control flow below reproduces
    // exactly that ordering. A `bind` failure runs only the `errdefer destroy`
    // (the `defer unbind` is not yet registered).
    pub fn init(opts: Options, len: usize) -> Result<Self, errors::Error> {
        let buffer = GlBuffer::create()?;

        let binding = match buffer.bind(opts.target) {
            Ok(b) => b,
            Err(e) => {
                buffer.destroy(); // errdefer
                return Err(e);
            }
        };

        let res = binding.set_data_null_manual(len * size_of::<T>(), opts.usage);
        binding.unbind(); // defer
        if let Err(e) = res {
            buffer.destroy(); // errdefer
            return Err(e);
        }

        Ok(Self {
            buffer,
            opts,
            len,
            _marker: PhantomData,
        })
    }

    /// Init the buffer filled with the given data.
    //
    // PORTS: `buffer.zig:50-64`.
    //
    // ```zig
    // pub fn initFill(opts: Options, data: []const T) !Self {
    //     const buffer = try gl.Buffer.create();
    //     errdefer buffer.destroy();
    //     const binding = try buffer.bind(opts.target);
    //     defer binding.unbind();
    //     try binding.setData(data, opts.usage);
    //     return .{ .buffer = buffer, .opts = opts, .len = data.len * @sizeOf(T) };
    // }
    // ```
    pub fn init_fill(opts: Options, data: &[T]) -> Result<Self, errors::Error> {
        let buffer = GlBuffer::create()?;

        let binding = match buffer.bind(opts.target) {
            Ok(b) => b,
            Err(e) => {
                buffer.destroy(); // errdefer
                return Err(e);
            }
        };

        let res = binding.set_data(data, opts.usage);
        binding.unbind(); // defer
        if let Err(e) = res {
            buffer.destroy(); // errdefer
            return Err(e);
        }

        Ok(Self {
            buffer,
            opts,
            // FAITHFULNESS QUIRK (R1): `data.len * @sizeOf(T)` — BYTES, not the
            // element count `init` stores. See the `len` field note; reproduced
            // exactly, NOT corrected. (`size_of_val(data)` is definitionally
            // `data.len * @sizeOf(T)` for a `&[T]` — the clippy-preferred spelling,
            // same as the Metal twin's `init_fill`.)
            // RE-AFFIRMED (P3-W0 T5): checked against oracle range 4749c4e93..6ad1fe7d8 — src/renderer/opengl/buffer.zig is UNTOUCHED there, so this quirk is still faithful. Ledger: .port/tools/gates/stale-comments-D9.tsv.
            len: size_of_val(data),
            _marker: PhantomData,
        })
    }

    /// PORTS: `buffer.zig:66-68`.
    ///
    /// ```zig
    /// pub fn deinit(self: Self) void {
    ///     self.buffer.destroy();
    /// }
    /// ```
    ///
    /// PORT NOTE: consumes `self` (by value) — NOT a `Drop` impl. See the module
    /// note: the reference manages the GL handle explicitly, and this wrapper is
    /// deliberately not `Copy`, so consuming `self` here is the faithful shape.
    pub fn deinit(self) {
        self.buffer.destroy();
    }

    /// Sync new contents to the buffer. The data is expected to be the
    /// complete contents of the buffer. If the amount of data is larger
    /// than the buffer length, the buffer will be reallocated.
    ///
    /// If the amount of data is smaller than the buffer length, the
    /// remaining data in the buffer is left untouched.
    //
    // PORTS: `buffer.zig:76-92`.
    //
    // ```zig
    // pub fn sync(self: *Self, data: []const T) !void {
    //     const binding = try self.buffer.bind(self.opts.target);
    //     defer binding.unbind();
    //     if (data.len > self.len) {
    //         self.len = data.len * 2;
    //         try binding.setDataNullManual(self.len * @sizeOf(T), self.opts.usage);
    //     }
    //     try binding.setSubData(0, data);
    // }
    // ```
    //
    // PORT NOTE (`binding` holds no borrow of `self`): `gl::Buffer::bind` takes the
    // `Copy` handle by value and returns a `Binding` that carries only copied
    // `{id, target}` values — so mutating `self.len` after the bind is sound. The
    // `defer binding.unbind()` is reproduced on every post-bind exit path.
    pub fn sync(&mut self, data: &[T]) -> Result<(), errors::Error> {
        let binding = self.buffer.bind(self.opts.target)?;

        // If we need more space than our buffer has, we need to reallocate.
        if data.len() > self.len {
            // Reallocate the buffer to hold double what we require.
            self.len = data.len() * 2;
            if let Err(e) = binding.set_data_null_manual(self.len * size_of::<T>(), self.opts.usage)
            {
                binding.unbind(); // defer
                return Err(e);
            }
        }

        // We can fit within the buffer so we can just replace bytes.
        let res = binding.set_sub_data(0, data);
        binding.unbind(); // defer
        res
    }

    /// Like Buffer.sync but takes data from an array of ArrayLists,
    /// rather than a single array. Returns the number of items synced.
    //
    // PORTS: `buffer.zig:96-124`.
    //
    // ```zig
    // pub fn syncFromArrayLists(self: *Self, lists: []const std.ArrayListUnmanaged(T)) !usize {
    //     const binding = try self.buffer.bind(self.opts.target);
    //     defer binding.unbind();
    //     var total_len: usize = 0;
    //     for (lists) |list| total_len += list.items.len;
    //     if (total_len > self.len) {
    //         self.len = total_len * 2;
    //         try binding.setDataNullManual(self.len * @sizeOf(T), self.opts.usage);
    //     }
    //     var i: usize = 0;
    //     for (lists) |list| {
    //         try binding.setSubData(i, list.items);
    //         i += list.items.len * @sizeOf(T);
    //     }
    //     return total_len;
    // }
    // ```
    //
    // PORT NOTE (`[]const std.ArrayListUnmanaged(T)` -> `&[Vec<T>]`): same mapping
    // as the Metal twin — the already-ported `ArrayListCollection<T>.lists` is a
    // `Vec<Vec<T>>`, so an array of array-lists is a `&[Vec<T>]`.
    pub fn sync_from_array_lists(&mut self, lists: &[Vec<T>]) -> Result<usize, errors::Error> {
        let binding = self.buffer.bind(self.opts.target)?;

        let mut total_len: usize = 0;
        for list in lists {
            total_len += list.len();
        }

        // If we need more space than our buffer has, we need to reallocate.
        if total_len > self.len {
            // Reallocate the buffer to hold double what we require.
            self.len = total_len * 2;
            if let Err(e) = binding.set_data_null_manual(self.len * size_of::<T>(), self.opts.usage)
            {
                binding.unbind(); // defer
                return Err(e);
            }
        }

        // We can fit within the buffer so we can just replace bytes.
        let mut i: usize = 0;
        for list in lists {
            if let Err(e) = binding.set_sub_data(i, list) {
                binding.unbind(); // defer
                return Err(e);
            }
            i += list.len() * size_of::<T>();
        }

        binding.unbind(); // defer
        Ok(total_len)
    }
}
