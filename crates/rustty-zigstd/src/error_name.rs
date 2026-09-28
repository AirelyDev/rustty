//! Zig's `{}` of an error value: `error.<Name>`.
//!
//! PORTS the rendering, not a type: `std.fmt` prints an error value under `{}`
//! as `error.` plus `@errorName(err)` (MEASURED with Zig 0.16.0:
//! `std.debug.print("{}", .{error.FileNotFound})` prints `error.FileNotFound`).
//! The reference's diagnostics print caught errors that way; the port's error
//! enums implement [`ErrorName`] with the reference's names, and the sites print
//! [`ZigError`] (NEW-FINDINGS N121, N11). An error type whose Zig error set
//! nests others (the port's `Face(FaceError)`-style wrappers) returns the name
//! of the error it wraps, since a Zig error union is flat.

use core::fmt;
use std::borrow::Cow;

/// An error value whose Zig name is known.
pub trait ErrorName {
    /// `@errorName(err)`: the name without the `error.` prefix.
    fn error_name(&self) -> Cow<'static, str>;
}

/// `{}` of a Zig error: `error.<Name>`.
pub struct ZigError<'a, E: ?Sized>(pub &'a E);

impl<E: ErrorName + ?Sized> fmt::Display for ZigError<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error.{}", self.0.error_name())
    }
}

/// The name of a unit-only enum whose variant names ARE the reference's error
/// names: its `Debug` rendering. Only for enums with no data-carrying variant
/// (a tuple or struct variant's `Debug` is not a name).
pub fn unit_variant_name(e: &impl fmt::Debug) -> Cow<'static, str> {
    Cow::Owned(format!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    enum Leaf {
        FileNotFound,
    }
    impl ErrorName for Leaf {
        fn error_name(&self) -> Cow<'static, str> {
            unit_variant_name(self)
        }
    }
    enum Wrapper {
        Leaf(Leaf),
        OutOfMemory,
    }
    impl ErrorName for Wrapper {
        fn error_name(&self) -> Cow<'static, str> {
            match self {
                Wrapper::Leaf(l) => l.error_name(),
                Wrapper::OutOfMemory => Cow::Borrowed("OutOfMemory"),
            }
        }
    }

    #[test]
    fn renders_error_dot_name_and_flattens_wrappers() {
        assert_eq!(
            "error.FileNotFound",
            ZigError(&Leaf::FileNotFound).to_string()
        );
        assert_eq!(
            "error.FileNotFound",
            ZigError(&Wrapper::Leaf(Leaf::FileNotFound)).to_string()
        );
        assert_eq!(
            "error.OutOfMemory",
            ZigError(&Wrapper::OutOfMemory).to_string()
        );
    }
}
