//! PORTS: ghostty `pkg/opengl/Buffer.zig` (245 loc).
//!
//! PORT NOTE (`glad.context.<Fn>.?(…)`): see `draw.rs`'s module note — the
//! reference force-unwraps every glad slot, the `gl` crate panics identically on
//! an unloaded entry point, so the calls go straight through.
//!
//! PORT NOTE (by-value `self`): the reference's methods take `self: Buffer` /
//! `b: Binding` BY VALUE — both are single-`GLuint`-or-smaller plain structs.
//! [`Buffer`] and [`Binding`] are therefore `Copy` and their methods take `self`
//! by value, so `buf.bind(…)` followed by `buf.destroy()` compiles here exactly
//! as it does there (`renderer/opengl/buffer.zig:34-47`).
//!
//! PORT NOTE (`Binding.attribute`, `Buffer.zig:139-179`): NOT PORTED, and
//! deliberately so. Its body ends in `try b.enableAttribArray(idx)` — but
//! `Buffer.Binding` HAS NO `enableAttribArray` method (only `VertexArray.Binding`
//! does, `VertexArray.zig:33`). Zig type-checks a body containing `comptime T:
//! type` only when it is instantiated, and this one has no call site anywhere in
//! the reference, so the mistake has never been compiled. Its `@typeInfo` switch
//! is stale in the same way `draw.zig`'s `pixelStore` is (`.Array`, a pre-0.14
//! tag spelling, vs `dataInfo`'s current `.pointer`/`.slice` two lines away).
//! Porting broken, uninstantiable, uncalled code would mean INVENTING the
//! behaviour it never had, which the plan forbids; its three helpers
//! ([`Binding::attribute_divisor`], [`Binding::attribute_advanced`],
//! [`Binding::attribute_i_advanced`] — its only callers in the reference) ARE
//! ported. The live vertex-attribute path is `VertexArray.Binding`'s
//! separate-format API, which is fully ported in `vertex_array.rs`.

use std::ffi::c_void;

use gl::types::{GLenum, GLint, GLsizei, GLsizeiptr, GLuint};

use super::errors;
use super::glad;

/// PORTS: `Buffer.zig:1-8` — `const Buffer = @This(); … id: c.GLuint`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Buffer {
    pub id: GLuint,
}

impl Buffer {
    /// Create a single buffer.
    //
    // PORTS: `Buffer.zig:10-15`.
    //
    // ```zig
    // pub fn create() !Buffer {
    //     var vbo: c.GLuint = undefined;
    //     glad.context.GenBuffers.?(1, &vbo);
    //     return Buffer{ .id = vbo };
    // }
    // ```
    //
    // PORT NOTE (`!Buffer` with no `getError`): the inferred error set is EMPTY —
    // this cannot fail. The `Result` is kept so the reference's `try
    // gl.Buffer.create()` call sites (`renderer/opengl/buffer.zig:34,51`) port
    // with their `?` intact. Same handling as `draw::viewport`.
    //
    // PORT NOTE (`var vbo: c.GLuint = undefined`): Rust has no safe `undefined`,
    // so the slot is zero-initialized. Behaviourally identical — `glGenBuffers`
    // unconditionally overwrites it — and it is not even observable, since the
    // value is read only after the call.
    pub fn create() -> Result<Self, errors::Error> {
        let mut vbo: GLuint = 0;
        // SAFETY: `glGenBuffers` writes exactly `n` (= 1) GLuints through the
        // pointer; `&mut vbo` is one live, aligned GLuint.
        glad::context(|gl| unsafe { gl.GenBuffers(1, &mut vbo) });
        Ok(Buffer { id: vbo })
    }

    /// glBindBuffer
    //
    // PORTS: `Buffer.zig:17-21`.
    //
    // ```zig
    // pub fn bind(self: Buffer, target: Target) !Binding {
    //     glad.context.BindBuffer.?(@intFromEnum(target), self.id);
    //     return Binding{ .id = self.id, .target = target };
    // }
    // ```
    //
    // PORT NOTE: another empty-error-set `!Binding` — see `create`.
    pub fn bind(self, target: Target) -> Result<Binding, errors::Error> {
        // SAFETY: binds a name this `Buffer` owns to a GL target. A bad target is
        // GL_INVALID_ENUM (which the reference does not collect here), never UB.
        glad::context(|gl| unsafe { gl.BindBuffer(target.0, self.id) });
        Ok(Binding {
            id: self.id,
            target,
        })
    }

    /// PORTS: `Buffer.zig:23-25`.
    ///
    /// ```zig
    /// pub fn destroy(self: Buffer) void {
    ///     glad.context.DeleteBuffers.?(1, &self.id);
    /// }
    /// ```
    ///
    /// PORT NOTE: takes `self` BY VALUE, mirroring the reference — this is NOT a
    /// `Drop` impl. The reference manages GL object lifetimes explicitly
    /// (`errdefer buffer.destroy()`, `renderer/opengl/buffer.zig:35`), and
    /// `Buffer` is `Copy`, so an RAII `Drop` would be both a deviation and
    /// unsound (a copy would double-delete).
    pub fn destroy(self) {
        // SAFETY: `glDeleteBuffers` reads exactly `n` (= 1) GLuints through the
        // pointer; `&self.id` is one live, aligned GLuint. Deleting an already-
        // deleted or unknown name is silently ignored by GL, not UB.
        glad::context(|gl| unsafe { gl.DeleteBuffers(1, &self.id) });
    }

    /// PORTS: `Buffer.zig:27-34`.
    ///
    /// ```zig
    /// pub fn bindBase(self: Buffer, target: Target, idx: c.GLuint) !void {
    ///     glad.context.BindBufferBase.?(
    ///         @intFromEnum(target),
    ///         idx,
    ///         self.id,
    ///     );
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// The uniform/storage-block binding path: `renderer/opengl/RenderPass.zig:95`
    /// (`ubo.bindBase(.uniform, 1)`) and `:120` (`buf.bindBase(.storage, i)`).
    ///
    /// PORT NOTE: the brief lists `bind_base` under `Binding`; it is a method on
    /// `Buffer` in the reference (it needs no prior bind), and is ported where the
    /// reference puts it.
    pub fn bind_base(self, target: Target, idx: GLuint) -> Result<(), errors::Error> {
        // SAFETY: an indexed bind of a name this `Buffer` owns. An out-of-range
        // index or a non-indexed target is a defined GL error, collected below.
        glad::context(|gl| unsafe { gl.BindBufferBase(target.0, idx, self.id) });
        errors::get_error()
    }
}

/// Binding is a bound buffer. By using this for functions that operate
/// on bound buffers, you can easily defer unbinding and in safety-enabled
/// modes verify that unbound buffers are never accessed.
//
// PORTS: `Buffer.zig:36-222`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Binding {
    pub id: GLuint,
    pub target: Target,
}

impl Binding {
    /// PORTS: `Buffer.zig:43-45`.
    ///
    /// ```zig
    /// pub fn unbind(b: Binding) void {
    ///     glad.context.BindBuffer.?(@intFromEnum(b.target), 0);
    /// }
    /// ```
    pub fn unbind(self) {
        // SAFETY: binding name 0 to a valid target always succeeds.
        glad::context(|gl| unsafe { gl.BindBuffer(self.target.0, 0) });
    }

    /// Sets the data of this bound buffer. The data can be any array-like
    /// type. The size of the data is automatically determined based on the type.
    //
    // PORTS: `Buffer.zig:47-62`.
    //
    // ```zig
    // pub fn setData(
    //     b: Binding,
    //     data: anytype,
    //     usage: Usage,
    // ) !void {
    //     const info = dataInfo(data);
    //     glad.context.BufferData.?(
    //         @intFromEnum(b.target),
    //         info.size,
    //         info.ptr,
    //         @intFromEnum(usage),
    //     );
    //     try errors.getError();
    // }
    // ```
    //
    // PORT NOTE (`data: anytype` -> `data: &[T]`): see [`data_info`].
    pub fn set_data<T>(self, data: &[T], usage: Usage) -> Result<(), errors::Error> {
        let info = data_info(data);
        // SAFETY: `info.ptr`/`info.size` describe `data`'s own bytes exactly (see
        // `data_info`), and GL copies them before returning — it retains nothing.
        glad::context(|gl| unsafe { gl.BufferData(self.target.0, info.size, info.ptr, usage.0) });
        errors::get_error()
    }

    /// Sets the data of this bound buffer. The data can be any array-like
    /// type. The size of the data is automatically determined based on the type.
    //
    // PORTS: `Buffer.zig:64-79`.
    //
    // ```zig
    // pub fn setSubData(
    //     b: Binding,
    //     offset: usize,
    //     data: anytype,
    // ) !void {
    //     const info = dataInfo(data);
    //     glad.context.BufferSubData.?(
    //         @intFromEnum(b.target),
    //         @intCast(offset),
    //         info.size,
    //         info.ptr,
    //     );
    //     try errors.getError();
    // }
    // ```
    //
    // (The doc comment repeating `setData`'s is the reference's own.)
    pub fn set_sub_data<T>(self, offset: usize, data: &[T]) -> Result<(), errors::Error> {
        let info = data_info(data);
        // `@intCast(offset)`: usize -> GLintptr (isize). Zig traps on overflow;
        // `try_from` panics on it, which is the same contract. Callers derive the
        // offset from a live buffer's byte length, so it cannot exceed isize::MAX.
        let offset = gl::types::GLintptr::try_from(offset)
            .expect("Buffer.Binding::set_sub_data offset exceeds GLintptr");
        // SAFETY: see `set_data`. An offset+size past the store's end is
        // GL_INVALID_VALUE, collected below — GL bounds-checks against the size
        // it was given, it does not read past `data`.
        glad::context(|gl| unsafe { gl.BufferSubData(self.target.0, offset, info.size, info.ptr) });
        errors::get_error()
    }

    /// Sets the buffer data with a null buffer that is expected to be
    /// filled in the future using subData. This requires the type just so
    /// we can setup the data size.
    //
    // PORTS: `Buffer.zig:81-96`.
    //
    // ```zig
    // pub fn setDataNull(
    //     b: Binding,
    //     comptime T: type,
    //     usage: Usage,
    // ) !void {
    //     glad.context.BufferData.?(
    //         @intFromEnum(b.target),
    //         @sizeOf(T),
    //         null,
    //         @intFromEnum(usage),
    //     );
    //     try errors.getError();
    // }
    // ```
    //
    // PORT NOTE (`comptime T: type` -> a generic parameter): the one place in
    // this file where Zig's comptime maps onto Rust generics with no friction —
    // `T` is used ONLY for `@sizeOf(T)`, which is `size_of::<T>()`. Called as
    // `binding.set_data_null::<Uniforms>(usage)`.
    //
    // PORT NOTE: unused by the reference (`renderer/opengl/buffer.zig` calls only
    // `setDataNullManual`); carried over per "port whole files".
    pub fn set_data_null<T>(self, usage: Usage) -> Result<(), errors::Error> {
        // SAFETY: a null `data` pointer is explicitly legal for `glBufferData` —
        // it allocates the store and leaves the contents undefined, which is this
        // fn's whole purpose.
        glad::context(|gl| unsafe {
            gl.BufferData(
                self.target.0,
                size_of::<T>() as GLsizeiptr,
                std::ptr::null(),
                usage.0,
            );
        });
        errors::get_error()
    }

    /// Same as setDataNull but lets you manually specify the buffer size.
    //
    // PORTS: `Buffer.zig:98-111`.
    //
    // ```zig
    // pub fn setDataNullManual(
    //     b: Binding,
    //     size: usize,
    //     usage: Usage,
    // ) !void {
    //     glad.context.BufferData.?(
    //         @intFromEnum(b.target),
    //         @intCast(size),
    //         null,
    //         @intFromEnum(usage),
    //     );
    //     try errors.getError();
    // }
    // ```
    //
    // The prealloc/grow path: `renderer/opengl/buffer.zig:40,84,109` (Task 5).
    pub fn set_data_null_manual(self, size: usize, usage: Usage) -> Result<(), errors::Error> {
        // `@intCast(size)`: usize -> GLsizeiptr (isize), trapping on overflow in
        // Zig; `try_from` panics identically.
        let size = GLsizeiptr::try_from(size)
            .expect("Buffer.Binding::set_data_null_manual size exceeds GLsizeiptr");
        // SAFETY: see `set_data_null`.
        glad::context(|gl| unsafe {
            gl.BufferData(self.target.0, size, std::ptr::null(), usage.0)
        });
        errors::get_error()
    }

    /// VertexAttribDivisor
    //
    // PORTS: `Buffer.zig:181-185`.
    //
    // ```zig
    // pub fn attributeDivisor(_: Binding, idx: c.GLuint, divisor: c.GLuint) !void {
    //     glad.context.VertexAttribDivisor.?(idx, divisor);
    //     try errors.getError();
    // }
    // ```
    //
    // PORT NOTE (`_: Binding`): the reference DISCARDS the receiver — the call
    // acts on the current VAO, not on this buffer; the `Binding` is a bind-state
    // token only. Rust only grants method-call syntax to a receiver literally
    // named `self`, so it is named and simply goes unused — the same shape
    // `VertexArray.zig:28-31`'s `unbind(self: Binding) { _ = self; … }` uses.
    //
    // PORT NOTE: the reference's only caller is the uninstantiable
    // `Binding.attribute` (see the module docs), so this is unused; carried over
    // per "port whole files". The live divisor path is
    // `VertexArray.Binding.bindingDivisor` (`vertex_array.rs`).
    pub fn attribute_divisor(self, idx: GLuint, divisor: GLuint) -> Result<(), errors::Error> {
        // SAFETY: acts on the currently-bound VAO's attribute state; a bad index
        // is GL_INVALID_VALUE, collected below.
        glad::context(|gl| unsafe { gl.VertexAttribDivisor(idx, divisor) });
        errors::get_error()
    }

    /// PORTS: `Buffer.zig:187-204`.
    ///
    /// ```zig
    /// pub fn attributeAdvanced(
    ///     _: Binding,
    ///     idx: c.GLuint,
    ///     size: c.GLint,
    ///     typ: c.GLenum,
    ///     normalized: bool,
    ///     stride: c.GLsizei,
    ///     offset: usize,
    /// ) !void {
    ///     const normalized_c: c.GLboolean = if (normalized) c.GL_TRUE else c.GL_FALSE;
    ///     const offsetPtr = if (offset > 0)
    ///         @as(*const anyopaque, @ptrFromInt(offset))
    ///     else
    ///         null;
    ///
    ///     glad.context.VertexAttribPointer.?(idx, size, typ, normalized_c, stride, offsetPtr);
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE: see [`Binding::attribute_divisor`] — same discarded receiver,
    /// same "unused because its only caller is uninstantiable" status.
    pub fn attribute_advanced(
        self,
        idx: GLuint,
        size: GLint,
        typ: GLenum,
        normalized: bool,
        stride: GLsizei,
        offset: usize,
    ) -> Result<(), errors::Error> {
        let normalized_c: gl::types::GLboolean = if normalized { gl::TRUE } else { gl::FALSE };
        // `@ptrFromInt(offset)`: with an ARRAY_BUFFER bound, GL reads this
        // "pointer" as a byte offset into that buffer. See `draw::draw_elements`.
        let offset_ptr: *const c_void = if offset > 0 {
            offset as *const c_void
        } else {
            std::ptr::null()
        };

        // SAFETY: acts on the currently-bound VAO + ARRAY_BUFFER. Every failure
        // mode is a defined GL error, collected below.
        glad::context(|gl| unsafe {
            gl.VertexAttribPointer(idx, size, typ, normalized_c, stride, offset_ptr)
        });
        errors::get_error()
    }

    /// PORTS: `Buffer.zig:206-221`.
    ///
    /// ```zig
    /// pub fn attributeIAdvanced(
    ///     _: Binding,
    ///     idx: c.GLuint,
    ///     size: c.GLint,
    ///     typ: c.GLenum,
    ///     stride: c.GLsizei,
    ///     offset: usize,
    /// ) !void {
    ///     const offsetPtr = if (offset > 0)
    ///         @as(*const anyopaque, @ptrFromInt(offset))
    ///     else
    ///         null;
    ///
    ///     glad.context.VertexAttribIPointer.?(idx, size, typ, stride, offsetPtr);
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE: see [`Binding::attribute_divisor`].
    pub fn attribute_i_advanced(
        self,
        idx: GLuint,
        size: GLint,
        typ: GLenum,
        stride: GLsizei,
        offset: usize,
    ) -> Result<(), errors::Error> {
        let offset_ptr: *const c_void = if offset > 0 {
            offset as *const c_void
        } else {
            std::ptr::null()
        };

        // SAFETY: see `attribute_advanced`.
        glad::context(|gl| unsafe { gl.VertexAttribIPointer(idx, size, typ, stride, offset_ptr) });
        errors::get_error()
    }
}

/// PORTS: `Buffer.zig:113-137`.
///
/// ```zig
/// fn dataInfo(data: anytype) struct {
///     size: isize,
///     ptr: *const anyopaque,
/// } {
///     return switch (@typeInfo(@TypeOf(data))) {
///         .pointer => |ptr| switch (ptr.size) {
///             .one => .{
///                 .size = @sizeOf(ptr.child),
///                 .ptr = data,
///             },
///             .slice => .{
///                 .size = @intCast(@sizeOf(ptr.child) * data.len),
///                 .ptr = data.ptr,
///             },
///             else => {
///                 std.log.err("invalid buffer data pointer size: {}", .{ptr.size});
///                 unreachable;
///             },
///         },
///         else => {
///             std.log.err("invalid buffer data type: {s}", .{@tagName(@typeInfo(@TypeOf(data)))});
///             unreachable;
///         },
///     };
/// }
/// ```
///
/// PORT NOTE (`data: anytype` -> `data: &[T]`): the reference's switch has two
/// live arms and two `unreachable` ones. Rust's coherence rules cannot express
/// "a pointer to one T OR a slice of T" as one parameter without inventing a
/// trait, which the plan forbids — so the arm that is ported is the one the
/// reference actually reaches: `.slice`. Every `setData`/`setSubData` call site
/// in the reference passes a slice (`renderer/opengl/buffer.zig:57` `data:
/// []const T`, `:91` `setSubData(0, data)`, `:119` `setSubData(i, list.items)`);
/// there is NO `.one` call site. The two `unreachable` arms need no port at all:
/// they trap at runtime on a type Rust rejects at compile time, which is the same
/// contract enforced earlier.
///
/// PORT NOTE: `.one` is recoverable at any later task with zero API churn —
/// `binding.set_data(std::slice::from_ref(&value), usage)` is byte-for-byte the
/// call the `.one` arm would have made.
struct DataInfo {
    size: GLsizeiptr,
    ptr: *const c_void,
}

fn data_info<T>(data: &[T]) -> DataInfo {
    DataInfo {
        // `@intCast(@sizeOf(ptr.child) * data.len)`: usize -> isize, trapping on
        // overflow in Zig. It cannot fire here for a stronger reason than in the
        // reference: Rust GUARANTEES no single allocation exceeds `isize::MAX`
        // bytes, so a live `&[T]`'s byte length is always a valid `GLsizeiptr`.
        size: size_of_val(data) as GLsizeiptr,
        // `.ptr = data.ptr`. A zero-length slice yields a dangling-but-aligned
        // pointer, which GL never dereferences because it is paired with size 0.
        ptr: data.as_ptr().cast::<c_void>(),
    }
}

/// Enum for possible binding targets.
//
// PORTS: `Buffer.zig:224-231`.
//
// ```zig
// pub const Target = enum(c_uint) {
//     array = c.GL_ARRAY_BUFFER,
//     element_array = c.GL_ELEMENT_ARRAY_BUFFER,
//     uniform = c.GL_UNIFORM_BUFFER,
//     storage = c.GL_SHADER_STORAGE_BUFFER,
//     _,
// };
// ```
//
// PORT NOTE (the trailing `_`): this is a Zig NON-EXHAUSTIVE enum — any `c_uint`
// is a valid value, the four named ones are merely the convenient spellings. A
// Rust `enum` cannot model that (a value outside its variants is instant UB), so
// it is ported as a newtype over the tag type with the named values as associated
// constants. That is the tree's established handling for a trailing `_`, per
// `metal/api.rs:29-33`: "the trailing `_` = a non-exhaustive open enum, which the
// NSUInteger newtype models natively". `@intFromEnum(target)` becomes `target.0`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Target(pub GLenum);

impl Target {
    pub const ARRAY: Self = Self(gl::ARRAY_BUFFER);
    pub const ELEMENT_ARRAY: Self = Self(gl::ELEMENT_ARRAY_BUFFER);
    pub const UNIFORM: Self = Self(gl::UNIFORM_BUFFER);
    pub const STORAGE: Self = Self(gl::SHADER_STORAGE_BUFFER);
}

/// Enum for possible buffer usages.
//
// PORTS: `Buffer.zig:233-245`.
//
// ```zig
// pub const Usage = enum(c_uint) {
//     stream_draw = c.GL_STREAM_DRAW,
//     stream_read = c.GL_STREAM_READ,
//     stream_copy = c.GL_STREAM_COPY,
//     static_draw = c.GL_STATIC_DRAW,
//     static_read = c.GL_STATIC_READ,
//     static_copy = c.GL_STATIC_COPY,
//     dynamic_draw = c.GL_DYNAMIC_DRAW,
//     dynamic_read = c.GL_DYNAMIC_READ,
//     dynamic_copy = c.GL_DYNAMIC_COPY,
//     _,
// };
// ```
//
// PORT NOTE (the trailing `_`): see [`Target`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Usage(pub GLenum);

impl Usage {
    pub const STREAM_DRAW: Self = Self(gl::STREAM_DRAW);
    pub const STREAM_READ: Self = Self(gl::STREAM_READ);
    pub const STREAM_COPY: Self = Self(gl::STREAM_COPY);
    pub const STATIC_DRAW: Self = Self(gl::STATIC_DRAW);
    pub const STATIC_READ: Self = Self(gl::STATIC_READ);
    pub const STATIC_COPY: Self = Self(gl::STATIC_COPY);
    pub const DYNAMIC_DRAW: Self = Self(gl::DYNAMIC_DRAW);
    pub const DYNAMIC_READ: Self = Self(gl::DYNAMIC_READ);
    pub const DYNAMIC_COPY: Self = Self(gl::DYNAMIC_COPY);
}

#[cfg(test)]
mod tests {
    use super::{data_info, Target, Usage};

    /// RUSTTY-ONLY. `Target`/`Usage` are newtypes rather than the reference's
    /// `enum(c_uint)`, so the Zig compiler is no longer the thing guaranteeing
    /// each constant IS its `c.GL_*` value. Assert it. No GL context needed.
    #[test]
    fn target_and_usage_constants_are_the_gl_constants() {
        assert_eq!(Target::ARRAY.0, gl::ARRAY_BUFFER);
        assert_eq!(Target::ELEMENT_ARRAY.0, gl::ELEMENT_ARRAY_BUFFER);
        assert_eq!(Target::UNIFORM.0, gl::UNIFORM_BUFFER);
        assert_eq!(Target::STORAGE.0, gl::SHADER_STORAGE_BUFFER);

        assert_eq!(Usage::STREAM_DRAW.0, gl::STREAM_DRAW);
        assert_eq!(Usage::STREAM_READ.0, gl::STREAM_READ);
        assert_eq!(Usage::STREAM_COPY.0, gl::STREAM_COPY);
        assert_eq!(Usage::STATIC_DRAW.0, gl::STATIC_DRAW);
        assert_eq!(Usage::STATIC_READ.0, gl::STATIC_READ);
        assert_eq!(Usage::STATIC_COPY.0, gl::STATIC_COPY);
        assert_eq!(Usage::DYNAMIC_DRAW.0, gl::DYNAMIC_DRAW);
        assert_eq!(Usage::DYNAMIC_READ.0, gl::DYNAMIC_READ);
        assert_eq!(Usage::DYNAMIC_COPY.0, gl::DYNAMIC_COPY);
    }

    /// RUSTTY-ONLY. `dataInfo`'s `.slice` arm is the sizing rule every
    /// `setData`/`setSubData` in the reference depends on
    /// (`@sizeOf(ptr.child) * data.len`); getting it wrong silently uploads the
    /// wrong number of bytes rather than raising a GL error, so the smoke test
    /// could not catch it. No GL context needed.
    #[test]
    fn data_info_sizes_a_slice_by_element_size_times_len() {
        let floats = [1.0f32, 2.0, 3.0, 4.0];
        let info = data_info(&floats);
        assert_eq!(info.size, 16);
        assert_eq!(info.ptr, floats.as_ptr().cast());

        // A wider element type must scale the size, not the count.
        let doubles = [1.0f64, 2.0];
        assert_eq!(data_info(&doubles).size, 16);

        // The empty slice: size 0, and the pointer is never dereferenced by GL.
        let empty: [u32; 0] = [];
        assert_eq!(data_info(&empty).size, 0);
    }
}
