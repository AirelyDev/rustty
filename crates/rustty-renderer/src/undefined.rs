//! A field the reference leaves `undefined` until it is initialized.
//!
//! PORT NOTE (`2f0b65346`): both backends' `Shaders.uninit` (`opengl/shaders.zig`
//! and `metal/shaders.zig` at `12752b2ac`) set `.pipelines = undefined` (and, for
//! Metal, `.library = undefined`) with `.defunct = true`, so that the renderer can
//! be built before any GPU context exists and the shaders are created lazily on
//! the render thread. Rust has no `undefined`; [`Undefined<T>`] holds `None` for
//! it. Every read of an undefined value is illegal in Zig, and the reference
//! guards them all with `assert(!self.shaders.defunct)`, so reading an unset
//! `Undefined` panics (the port's "assert", R7). Reads and writes of a set value
//! go through `Deref`/`DerefMut`, so the pipeline accesses read exactly as the
//! reference's `self.shaders.pipelines.bg_color`.

/// A value that may still be the reference's `undefined`.
pub struct Undefined<T>(Option<T>);

impl<T> Undefined<T> {
    /// `= undefined`.
    pub const UNDEFINED: Undefined<T> = Undefined(None);

    /// A defined value.
    pub const fn new(value: T) -> Undefined<T> {
        Undefined(Some(value))
    }

    /// Whether the value is still `undefined`.
    pub fn is_undefined(&self) -> bool {
        self.0.is_none()
    }
}

impl<T> core::ops::Deref for Undefined<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0.as_ref().expect("read of an undefined value")
    }
}

impl<T> core::ops::DerefMut for Undefined<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.0.as_mut().expect("read of an undefined value")
    }
}

#[cfg(test)]
mod tests {
    use super::Undefined;

    #[test]
    fn a_defined_value_reads_through() {
        let mut v = Undefined::new(3);
        assert!(!v.is_undefined());
        *v += 1;
        assert_eq!(*v, 4);
    }

    #[test]
    #[should_panic(expected = "read of an undefined value")]
    fn reading_undefined_panics() {
        let v: Undefined<u8> = Undefined::UNDEFINED;
        assert!(v.is_undefined());
        let _ = *v;
    }
}
