// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/Context.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): ghostty's sprite canvas acquires a `Context` only to
// set the padding transformation; the actual drawing goes through `painter.fill`
// / `painter.stroke` directly. This subset therefore provides the `Context`
// surface state + transformation accessors it relies on. The full managed
// drawing API (`set_source`, `move_to`/`line_to`/`curve_to`/`arc`, `fill`,
// `stroke`, dashes, etc.) is a documented blocker.

//! Context patterns and state for drawing.

use crate::path::Path;
use crate::surface::Surface;
use crate::transformation::{Transformation, IDENTITY};

/// A managed drawing context bound to a mutable `Surface`.
pub struct Context<'a> {
    /// The surface this context draws to.
    pub surface: &'a mut Surface,
    /// The managed path builder.
    pub path: Path,
    /// The current transformation matrix (CTM).
    pub transformation: Transformation,
}

impl<'a> Context<'a> {
    /// Acquires a drawing context for `surface`.
    pub fn init(surface: &'a mut Surface) -> Context<'a> {
        Context {
            surface,
            path: Path::empty(),
            transformation: IDENTITY,
        }
    }

    /// Releases the context's managed path (Rust: dropped). The surface is not
    /// owned by the context and is left intact.
    pub fn deinit(&mut self) {
        self.path.reset();
    }

    /// Sets the current transformation matrix, also applying it to the managed
    /// path builder (mirroring z2d's `setTransformation`).
    pub fn set_transformation(&mut self, transformation: Transformation) {
        self.transformation = transformation;
        self.path.transformation = transformation;
    }

    /// Returns the current transformation matrix.
    pub fn get_transformation(&self) -> Transformation {
        self.transformation
    }
}
