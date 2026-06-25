//! This is a generated file, produced by nerd_font_codegen.py
//! DO NOT EDIT BY HAND!
//!
//! This file provides info extracted from the nerd fonts patcher script,
//! specifying the scaling/positioning attributes of various glyphs.
//
// PORT: ghostty src/font/nerd_font_attributes.zig (a generated lookup table).
// `const Constraint = @import("Glyph.zig").RenderOptions.Constraint;` maps to
// `crate::glyph::Constraint` (the reference's `RenderOptions.Constraint`). The
// Zig `switch (cp)` over `u21` becomes a Rust `match cp` over `u32`; each arm
// returns a `Constraint` built from `Constraint::none` with the listed fields
// overridden via struct-update syntax. Field/enum names are snake_case ports
// (e.g. `.fit_cover1` -> `ConstraintSize::FitCover1`, `.center1` -> `Align::Center1`).

// PORT NOTE: the float literals below are reproduced VERBATIM from the generated
// reference table (R1: comptime-generated artifacts are reproduced bit-for-bit).
// Several carry more decimal digits than the shortest f64 round-trip (e.g.
// `0.7187500000000000`); `clippy::excessive_precision` would have us truncate
// them, which would alter the faithful source text, so it is allowed here.
#![allow(clippy::excessive_precision)]

use crate::glyph::{Align, Constraint, ConstraintSize, Height};

/// Get the constraints for the provided codepoint.
// PORT NOTE: the reference takes `cp: u21`; Rust uses `u32` (codepoint), since
// it has no `u21`. The match patterns are identical hex ranges/values.
pub fn get_constraint(cp: u32) -> Option<Constraint> {
    match cp {
        0x2630 => Some(Constraint {
            size: ConstraintSize::Cover,
            height: Height::Icon,
            max_constraint_width: 1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            pad_left: 0.05,
            pad_right: 0.05,
            pad_top: 0.05,
            pad_bottom: 0.05,
            ..Constraint::none
        }),
        0x276c..=0x276d => Some(Constraint {
            size: ConstraintSize::Cover,
            max_constraint_width: 1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7142857142857143,
            relative_height: 0.8910614525139665,
            relative_x: 0.1428571428571428,
            relative_y: 0.0349162011173184,
            pad_top: 0.15,
            pad_bottom: 0.15,
            ..Constraint::none
        }),
        0x276e..=0x276f => Some(Constraint {
            size: ConstraintSize::Cover,
            max_constraint_width: 1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.9885714285714285,
            relative_height: 0.8910614525139665,
            relative_x: 0.0057142857142857,
            relative_y: 0.0125698324022346,
            pad_top: 0.15,
            pad_bottom: 0.15,
            ..Constraint::none
        }),
        0x2770..=0x2771 => Some(Constraint {
            size: ConstraintSize::Cover,
            max_constraint_width: 1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            pad_top: 0.15,
            pad_bottom: 0.15,
            ..Constraint::none
        }),
        0xe0a0..=0xe0a3 | 0xe0cf => Some(Constraint {
            size: ConstraintSize::FitCover1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            ..Constraint::none
        }),
        0xe0b0 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            pad_left: -0.03,
            pad_right: -0.03,
            pad_top: -0.005,
            pad_bottom: -0.005,
            max_xy_ratio: Some(0.7),
            ..Constraint::none
        }),
        0xe0b1 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            max_xy_ratio: Some(0.7),
            ..Constraint::none
        }),
        0xe0b2 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            pad_left: -0.03,
            pad_right: -0.03,
            pad_top: -0.005,
            pad_bottom: -0.005,
            max_xy_ratio: Some(0.7),
            ..Constraint::none
        }),
        0xe0b3 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            max_xy_ratio: Some(0.7),
            ..Constraint::none
        }),
        0xe0b4 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            pad_left: -0.03,
            pad_right: -0.03,
            pad_top: -0.005,
            pad_bottom: -0.005,
            max_xy_ratio: Some(0.59),
            ..Constraint::none
        }),
        0xe0b5 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            max_xy_ratio: Some(0.5),
            ..Constraint::none
        }),
        0xe0b6 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            pad_left: -0.03,
            pad_right: -0.03,
            pad_top: -0.005,
            pad_bottom: -0.005,
            max_xy_ratio: Some(0.59),
            ..Constraint::none
        }),
        0xe0b7 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            max_xy_ratio: Some(0.5),
            ..Constraint::none
        }),
        0xe0b8 | 0xe0bc => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            pad_left: -0.025,
            pad_right: -0.025,
            pad_top: -0.005,
            pad_bottom: -0.005,
            ..Constraint::none
        }),
        0xe0b9 | 0xe0bd => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            ..Constraint::none
        }),
        0xe0ba | 0xe0be => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            pad_left: -0.025,
            pad_right: -0.025,
            pad_top: -0.005,
            pad_bottom: -0.005,
            ..Constraint::none
        }),
        0xe0bb | 0xe0bf => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            ..Constraint::none
        }),
        0xe0c0 | 0xe0c8 => Some(Constraint {
            size: ConstraintSize::Stretch,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            pad_left: -0.025,
            pad_right: -0.025,
            pad_top: -0.005,
            pad_bottom: -0.005,
            ..Constraint::none
        }),
        0xe0c1 => Some(Constraint {
            size: ConstraintSize::Stretch,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            ..Constraint::none
        }),
        0xe0c2 | 0xe0ca => Some(Constraint {
            size: ConstraintSize::Stretch,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            pad_left: -0.025,
            pad_right: -0.025,
            pad_top: -0.005,
            pad_bottom: -0.005,
            ..Constraint::none
        }),
        0xe0c3 => Some(Constraint {
            size: ConstraintSize::Stretch,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            ..Constraint::none
        }),
        0xe0c4 => Some(Constraint {
            size: ConstraintSize::Stretch,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            pad_left: 0.015,
            pad_right: 0.015,
            pad_top: 0.015,
            pad_bottom: 0.015,
            max_xy_ratio: Some(0.86),
            ..Constraint::none
        }),
        0xe0c5 => Some(Constraint {
            size: ConstraintSize::Stretch,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            pad_left: 0.015,
            pad_right: 0.015,
            pad_top: 0.015,
            pad_bottom: 0.015,
            max_xy_ratio: Some(0.86),
            ..Constraint::none
        }),
        0xe0c6 => Some(Constraint {
            size: ConstraintSize::Stretch,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            pad_left: 0.015,
            pad_right: 0.015,
            pad_top: 0.015,
            pad_bottom: 0.015,
            max_xy_ratio: Some(0.78),
            ..Constraint::none
        }),
        0xe0c7 => Some(Constraint {
            size: ConstraintSize::Stretch,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            pad_left: 0.015,
            pad_right: 0.015,
            pad_top: 0.015,
            pad_bottom: 0.015,
            max_xy_ratio: Some(0.78),
            ..Constraint::none
        }),
        0xe0cc => Some(Constraint {
            size: ConstraintSize::Stretch,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            pad_left: -0.01,
            pad_right: -0.01,
            pad_top: -0.005,
            pad_bottom: -0.005,
            max_xy_ratio: Some(0.85),
            ..Constraint::none
        }),
        0xe0cd => Some(Constraint {
            size: ConstraintSize::Stretch,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            max_xy_ratio: Some(0.865),
            ..Constraint::none
        }),
        0xe0ce | 0xe0d0..=0xe0d1 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            ..Constraint::none
        }),
        0xe0d2 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            pad_left: -0.01,
            pad_right: -0.01,
            pad_top: -0.005,
            pad_bottom: -0.005,
            max_xy_ratio: Some(0.7),
            ..Constraint::none
        }),
        0xe0d4 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            pad_left: -0.01,
            pad_right: -0.01,
            pad_top: -0.005,
            pad_bottom: -0.005,
            max_xy_ratio: Some(0.7),
            ..Constraint::none
        }),
        0xe0d6 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            pad_left: -0.025,
            pad_right: -0.025,
            pad_top: -0.005,
            pad_bottom: -0.005,
            max_xy_ratio: Some(0.7),
            ..Constraint::none
        }),
        0xe0d7 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            pad_left: -0.025,
            pad_right: -0.025,
            pad_top: -0.005,
            pad_bottom: -0.005,
            max_xy_ratio: Some(0.7),
            ..Constraint::none
        }),
        0xe300 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8984375000000000,
            relative_y: 0.0986328125000000,
            ..Constraint::none
        }),
        0xe301 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8798828125000000,
            relative_y: 0.1171875000000000,
            ..Constraint::none
        }),
        0xe302 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7646484375000000,
            relative_y: 0.2314453125000000,
            ..Constraint::none
        }),
        0xe303 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8789062500000000,
            relative_y: 0.1171875000000000,
            ..Constraint::none
        }),
        0xe304 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9755859375000000,
            relative_y: 0.0244140625000000,
            ..Constraint::none
        }),
        0xe305 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9960937500000000,
            relative_y: 0.0019531250000000,
            ..Constraint::none
        }),
        0xe306 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9863281250000000,
            relative_y: 0.0097656250000000,
            ..Constraint::none
        }),
        0xe307 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9951171875000000,
            relative_y: 0.0039062500000000,
            ..Constraint::none
        }),
        0xe308 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9785156250000000,
            relative_y: 0.0195312500000000,
            ..Constraint::none
        }),
        0xe309 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9736328125000000,
            relative_y: 0.0214843750000000,
            ..Constraint::none
        }),
        0xe30a | 0xe35f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9648437500000000,
            relative_y: 0.0302734375000000,
            ..Constraint::none
        }),
        0xe30b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8437500000000000,
            relative_y: 0.1513671875000000,
            ..Constraint::none
        }),
        0xe30c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8027343750000000,
            relative_y: 0.1835937500000000,
            ..Constraint::none
        }),
        0xe30d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7753906250000000,
            relative_y: 0.1083984375000000,
            ..Constraint::none
        }),
        0xe30e | 0xe365 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9833984375000000,
            relative_y: 0.0166015625000000,
            ..Constraint::none
        }),
        0xe30f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9716796875000000,
            relative_y: 0.0263671875000000,
            ..Constraint::none
        }),
        0xe310 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6621093750000000,
            relative_y: 0.0986328125000000,
            ..Constraint::none
        }),
        0xe311 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6425781250000000,
            relative_y: 0.1171875000000000,
            ..Constraint::none
        }),
        0xe312 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5322265625000000,
            relative_y: 0.2314453125000000,
            ..Constraint::none
        }),
        0xe313 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6416015625000000,
            relative_y: 0.1181640625000000,
            ..Constraint::none
        }),
        0xe314 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7382812500000000,
            relative_y: 0.0195312500000000,
            ..Constraint::none
        }),
        0xe315 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6787109375000000,
            relative_y: 0.1357421875000000,
            ..Constraint::none
        }),
        0xe316 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7480468750000000,
            relative_y: 0.0097656250000000,
            ..Constraint::none
        }),
        0xe317 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7529296875000000,
            relative_y: 0.0048828125000000,
            ..Constraint::none
        }),
        0xe318 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7314453125000000,
            relative_y: 0.0263671875000000,
            ..Constraint::none
        }),
        0xe319 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7402343750000000,
            relative_y: 0.0195312500000000,
            ..Constraint::none
        }),
        0xe31a | 0xe35e => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7294921875000000,
            relative_y: 0.0283203125000000,
            ..Constraint::none
        }),
        0xe31b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6074218750000000,
            relative_y: 0.1503906250000000,
            ..Constraint::none
        }),
        0xe31c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7363281250000000,
            relative_y: 0.0224609375000000,
            ..Constraint::none
        }),
        0xe31d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7460937500000000,
            relative_y: 0.0126953125000000,
            ..Constraint::none
        }),
        0xe31e => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.2675781250000000,
            relative_y: 0.3310546875000000,
            ..Constraint::none
        }),
        0xe31f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7363281250000000,
            relative_y: 0.0986328125000000,
            ..Constraint::none
        }),
        0xe320 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7177734375000000,
            relative_y: 0.1171875000000000,
            ..Constraint::none
        }),
        0xe321 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8085937500000000,
            relative_y: 0.0253906250000000,
            ..Constraint::none
        }),
        0xe322 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7509765625000000,
            relative_y: 0.0839843750000000,
            ..Constraint::none
        }),
        0xe323 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8281250000000000,
            relative_y: 0.0097656250000000,
            ..Constraint::none
        }),
        0xe324 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8349609375000000,
            ..Constraint::none
        }),
        0xe325 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8154296875000000,
            relative_y: 0.0214843750000000,
            ..Constraint::none
        }),
        0xe326 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8144531250000000,
            relative_y: 0.0195312500000000,
            ..Constraint::none
        }),
        0xe327 | 0xe361 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8076171875000000,
            relative_y: 0.0273437500000000,
            ..Constraint::none
        }),
        0xe328 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6845703125000000,
            relative_y: 0.1503906250000000,
            ..Constraint::none
        }),
        0xe329 | 0xe367 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8173828125000000,
            relative_y: 0.0175781250000000,
            ..Constraint::none
        }),
        0xe32a => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8105468750000000,
            relative_y: 0.0263671875000000,
            ..Constraint::none
        }),
        0xe32b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5175781250000000,
            relative_y: 0.2421875000000000,
            ..Constraint::none
        }),
        0xe32c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6992187500000000,
            relative_y: 0.1005859375000000,
            ..Constraint::none
        }),
        0xe32d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6787109375000000,
            relative_y: 0.1201171875000000,
            ..Constraint::none
        }),
        0xe32e => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5654296875000000,
            relative_y: 0.2324218750000000,
            ..Constraint::none
        }),
        0xe32f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7714843750000000,
            relative_y: 0.0273437500000000,
            ..Constraint::none
        }),
        0xe330 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7148437500000000,
            relative_y: 0.0830078125000000,
            ..Constraint::none
        }),
        0xe331 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7919921875000000,
            relative_y: 0.0097656250000000,
            ..Constraint::none
        }),
        0xe332 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7871093750000000,
            relative_y: 0.0126953125000000,
            ..Constraint::none
        }),
        0xe333 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7714843750000000,
            relative_y: 0.0263671875000000,
            ..Constraint::none
        }),
        0xe334 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7773437500000000,
            relative_y: 0.0195312500000000,
            ..Constraint::none
        }),
        0xe335 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7714843750000000,
            relative_y: 0.0283203125000000,
            ..Constraint::none
        }),
        0xe336 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6503906250000000,
            relative_y: 0.1503906250000000,
            ..Constraint::none
        }),
        0xe337 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7753906250000000,
            relative_y: 0.0234375000000000,
            ..Constraint::none
        }),
        0xe338 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7792968750000000,
            relative_y: 0.0185546875000000,
            ..Constraint::none
        }),
        0xe339 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8445945945945946,
            ..Constraint::none
        }),
        0xe33a => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5283203125000000,
            relative_y: 0.2324218750000000,
            ..Constraint::none
        }),
        0xe33b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5449218750000000,
            relative_y: 0.2148437500000000,
            ..Constraint::none
        }),
        0xe33c..=0xe33d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5273437500000000,
            relative_y: 0.2324218750000000,
            ..Constraint::none
        }),
        0xe33e => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.3293918918918919,
            relative_y: 0.6706081081081081,
            ..Constraint::none
        }),
        0xe33f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5200000000000000,
            relative_y: 0.2707692307692308,
            ..Constraint::none
        }),
        0xe340 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8307692307692308,
            relative_y: 0.0861538461538462,
            ..Constraint::none
        }),
        0xe341 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8327702702702703,
            relative_y: 0.0050675675675676,
            ..Constraint::none
        }),
        0xe344 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5307692307692308,
            relative_y: 0.2092307692307692,
            ..Constraint::none
        }),
        0xe345 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5332112630208333,
            relative_y: 0.2040934244791667,
            ..Constraint::none
        }),
        0xe347 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8307692307692308,
            relative_y: 0.1246153846153846,
            ..Constraint::none
        }),
        0xe349 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5307967032967034,
            relative_y: 0.2615384615384616,
            ..Constraint::none
        }),
        0xe34c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8659995118379302,
            relative_y: 0.1340004881620698,
            ..Constraint::none
        }),
        0xe34d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9890163534293386,
            relative_y: 0.0002440810349036,
            ..Constraint::none
        }),
        0xe34f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5751953125000000,
            relative_y: 0.1142578125000000,
            ..Constraint::none
        }),
        0xe351 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6533203125000000,
            relative_y: 0.1328125000000000,
            ..Constraint::none
        }),
        0xe352 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5215384615384615,
            relative_y: 0.2846153846153846,
            ..Constraint::none
        }),
        0xe353 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8308012820512821,
            relative_y: 0.1230448717948718,
            ..Constraint::none
        }),
        0xe354..=0xe356 | 0xe358..=0xe359 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9935233160621761,
            relative_y: 0.0025906735751295,
            ..Constraint::none
        }),
        0xe357 | 0xe3a9 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9961139896373057,
            ..Constraint::none
        }),
        0xe35a => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9935233160621761,
            relative_y: 0.0012953367875648,
            ..Constraint::none
        }),
        0xe35b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9987046632124352,
            relative_y: 0.0012953367875648,
            ..Constraint::none
        }),
        0xe360 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7695312500000000,
            relative_y: 0.0302734375000000,
            ..Constraint::none
        }),
        0xe362 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9902343750000000,
            relative_y: 0.0097656250000000,
            ..Constraint::none
        }),
        0xe363 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7900390625000000,
            relative_y: 0.0097656250000000,
            ..Constraint::none
        }),
        0xe364 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8251953125000000,
            relative_y: 0.0097656250000000,
            ..Constraint::none
        }),
        0xe366 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7832031250000000,
            relative_y: 0.0166015625000000,
            ..Constraint::none
        }),
        0xe369 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.4902343750000000,
            relative_y: 0.2548828125000000,
            ..Constraint::none
        }),
        0xe36b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9333658774713205,
            relative_y: 0.0266048328044911,
            ..Constraint::none
        }),
        0xe36c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7076171875000000,
            relative_y: 0.1083984375000000,
            ..Constraint::none
        }),
        0xe36d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8427734375000000,
            relative_y: 0.0625000000000000,
            ..Constraint::none
        }),
        0xe36e => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7529721467391304,
            relative_y: 0.0956606657608696,
            ..Constraint::none
        }),
        0xe36f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6835937500000000,
            relative_y: 0.1250000000000000,
            ..Constraint::none
        }),
        0xe370 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8642578125000000,
            relative_y: 0.0625000000000000,
            ..Constraint::none
        }),
        0xe371 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6103515625000000,
            relative_y: 0.1933593750000000,
            ..Constraint::none
        }),
        0xe372 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7949218750000000,
            relative_y: 0.0576171875000000,
            ..Constraint::none
        }),
        0xe373 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8652343750000000,
            relative_y: 0.0058593750000000,
            ..Constraint::none
        }),
        0xe374 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.3154296875000000,
            relative_y: 0.2861328125000000,
            ..Constraint::none
        }),
        0xe375 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6772460937500000,
            relative_y: 0.1303710937500000,
            ..Constraint::none
        }),
        0xe376 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6992187500000000,
            relative_y: 0.1337890625000000,
            ..Constraint::none
        }),
        0xe377 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7314453125000000,
            relative_y: 0.1552734375000000,
            ..Constraint::none
        }),
        0xe378 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7314453125000000,
            relative_y: 0.1542968750000000,
            ..Constraint::none
        }),
        0xe379 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5751953125000000,
            relative_y: 0.1826171875000000,
            ..Constraint::none
        }),
        0xe37a => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5263671875000000,
            relative_y: 0.2285156250000000,
            ..Constraint::none
        }),
        0xe37b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5751953125000000,
            relative_y: 0.1835937500000000,
            ..Constraint::none
        }),
        0xe37d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9003906250000000,
            relative_y: 0.0957031250000000,
            ..Constraint::none
        }),
        0xe37e => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6015625000000000,
            relative_y: 0.2324218750000000,
            ..Constraint::none
        }),
        0xe37f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5200000000000000,
            relative_y: 0.2784615384615385,
            ..Constraint::none
        }),
        0xe380 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5200000000000000,
            relative_y: 0.2630769230769231,
            ..Constraint::none
        }),
        0xe38e..=0xe391 | 0xe394 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.4990253411306043,
            relative_height: 0.9987012987012988,
            relative_x: 0.4996751137102014,
            ..Constraint::none
        }),
        0xe392..=0xe393 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.4996751137102014,
            relative_height: 0.9987012987012988,
            relative_x: 0.4990253411306043,
            ..Constraint::none
        }),
        0xe395 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5471085120207927,
            relative_height: 0.9987012987012988,
            relative_x: 0.4515919428200130,
            ..Constraint::none
        }),
        0xe396 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5945419103313840,
            relative_height: 0.9987012987012988,
            relative_x: 0.4041585445094217,
            ..Constraint::none
        }),
        0xe397 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6426250812215725,
            relative_x: 0.3573749187784275,
            ..Constraint::none
        }),
        0xe398 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6900584795321637,
            relative_x: 0.3099415204678362,
            ..Constraint::none
        }),
        0xe399 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7381416504223521,
            relative_x: 0.2618583495776478,
            ..Constraint::none
        }),
        0xe39a => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7855750487329435,
            relative_x: 0.2144249512670565,
            ..Constraint::none
        }),
        0xe39b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.9987004548408057,
            relative_height: 0.9987012987012988,
            ..Constraint::none
        }),
        0xe39c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.8323586744639376,
            relative_height: 0.9935064935064936,
            ..Constraint::none
        }),
        0xe39d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7855750487329435,
            relative_height: 0.9948051948051948,
            ..Constraint::none
        }),
        0xe39e => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7381416504223521,
            relative_height: 0.9961038961038962,
            ..Constraint::none
        }),
        0xe39f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6907082521117609,
            relative_height: 0.9961038961038962,
            ..Constraint::none
        }),
        0xe3a0 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6426250812215725,
            relative_height: 0.9961038961038962,
            ..Constraint::none
        }),
        0xe3a1 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5945419103313840,
            relative_height: 0.9974025974025974,
            ..Constraint::none
        }),
        0xe3a2..=0xe3a3 | 0xe3a5 | 0xe3a7..=0xe3a8 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.4990253411306043,
            relative_height: 0.9987012987012988,
            ..Constraint::none
        }),
        0xe3a4 | 0xe3a6 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.4996751137102014,
            relative_height: 0.9987012987012988,
            ..Constraint::none
        }),
        0xe3aa => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9902343750000000,
            relative_y: 0.0078125000000000,
            ..Constraint::none
        }),
        0xe3ab => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7900390625000000,
            relative_y: 0.0058593750000000,
            ..Constraint::none
        }),
        0xe3ac => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8251953125000000,
            relative_y: 0.0078125000000000,
            ..Constraint::none
        }),
        0xe3ad => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7519531250000000,
            relative_y: 0.0068359375000000,
            ..Constraint::none
        }),
        0xe3ae => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6152343750000000,
            relative_y: 0.2324218750000000,
            ..Constraint::none
        }),
        0xe3af | 0xe3b3 | 0xe3b5..=0xe3bb => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9986072423398329,
            relative_y: 0.0013927576601671,
            ..Constraint::none
        }),
        0xe3b0..=0xe3b2 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9958217270194986,
            relative_y: 0.0041782729805014,
            ..Constraint::none
        }),
        0xe3c1 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6590187942396876,
            relative_y: 0.1349768123016842,
            ..Constraint::none
        }),
        0xe3c2 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7939956065413717,
            ..Constraint::none
        }),
        0x23fb..=0x23fe
        | 0x2665
        | 0x26a1
        | 0x2b58
        | 0xe000..=0xe00a
        | 0xe200..=0xe2a9
        | 0xe342..=0xe343
        | 0xe346
        | 0xe348
        | 0xe34a..=0xe34b
        | 0xe34e
        | 0xe350
        | 0xe35c..=0xe35d
        | 0xe368
        | 0xe36a
        | 0xe37c
        | 0xe381..=0xe38d
        | 0xe3b4
        | 0xe3bc..=0xe3c0
        | 0xe3c3..=0xe3e3
        | 0xe5fa..=0xe6b8
        | 0xe700..=0xe8ef
        | 0xea60
        | 0xea62..=0xea7c
        | 0xea7e..=0xea88
        | 0xea8a..=0xea8c
        | 0xea8f..=0xea98
        | 0xeaa3..=0xeab3
        | 0xeab8..=0xeac7
        | 0xeac9
        | 0xeacc..=0xead3
        | 0xead7..=0xeb09
        | 0xeb0b..=0xeb42
        | 0xeb44..=0xeb4e
        | 0xeb50..=0xeb6d
        | 0xeb72..=0xeb89
        | 0xeb8b..=0xeb99
        | 0xeb9b..=0xebd4
        | 0xebd7..=0xec06
        | 0xec08..=0xec0a
        | 0xec0d..=0xec1e
        | 0xed00..=0xedff
        | 0xee0c..=0xefce
        | 0xf000..=0xf004
        | 0xf006..=0xf025
        | 0xf028..=0xf02a
        | 0xf02c..=0xf030
        | 0xf034
        | 0xf036..=0xf043
        | 0xf045
        | 0xf047
        | 0xf053..=0xf05f
        | 0xf062
        | 0xf064..=0xf076
        | 0xf079..=0xf07d
        | 0xf07f..=0xf088
        | 0xf08a..=0xf0a3
        | 0xf0a6..=0xf0d6
        | 0xf0db
        | 0xf0df..=0xf0ff
        | 0xf108..=0xf12f
        | 0xf131..=0xf140
        | 0xf142..=0xf152
        | 0xf155
        | 0xf15a..=0xf174
        | 0xf176
        | 0xf179..=0xf181
        | 0xf183..=0xf220
        | 0xf223
        | 0xf22e..=0xf254
        | 0xf259
        | 0xf25c..=0xf381
        | 0xf400..=0xf415
        | 0xf417..=0xf423
        | 0xf425..=0xf430
        | 0xf435..=0xf437
        | 0xf439..=0xf43d
        | 0xf43f..=0xf442
        | 0xf446..=0xf449
        | 0xf44c..=0xf45b
        | 0xf45d..=0xf45f
        | 0xf462..=0xf466
        | 0xf468..=0xf46b
        | 0xf46d..=0xf46f
        | 0xf471..=0xf475
        | 0xf477..=0xf479
        | 0xf47f..=0xf48a
        | 0xf48c..=0xf492
        | 0xf494..=0xf499
        | 0xf49b..=0xf4c2
        | 0xf4c4..=0xf4ee
        | 0xf4f3..=0xf51c
        | 0xf51e..=0xf533
        | 0xf0001..=0xf1af0 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            ..Constraint::none
        }),
        0xea61 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7513020833333334,
            relative_height: 0.9291573452647278,
            relative_x: 0.0846354166666667,
            relative_y: 0.0708426547352722,
            ..Constraint::none
        }),
        0xea7d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.8394854586129754,
            relative_height: 0.8751387347391787,
            relative_x: 0.0917225950782998,
            relative_y: 0.0416204217536071,
            ..Constraint::none
        }),
        0xea99 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.9395973154362416,
            relative_height: 0.4778024417314096,
            relative_x: 0.0302013422818792,
            relative_y: 0.2269700332963374,
            ..Constraint::none
        }),
        0xea9a | 0xeaa1 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7673378076062640,
            relative_height: 0.8523862375138734,
            relative_x: 0.1526845637583893,
            relative_y: 0.0754716981132075,
            ..Constraint::none
        }),
        0xea9b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.8590604026845637,
            relative_height: 0.7613762486126526,
            relative_x: 0.0721476510067114,
            relative_y: 0.0871254162042175,
            ..Constraint::none
        }),
        0xea9c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.8590604026845637,
            relative_height: 0.7574916759156493,
            relative_x: 0.0721476510067114,
            relative_y: 0.0832408435072142,
            ..Constraint::none
        }),
        0xea9d | 0xeaa0 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.4082774049217002,
            relative_height: 0.5077691453940066,
            relative_x: 0.2863534675615212,
            relative_y: 0.2763596004439512,
            ..Constraint::none
        }),
        0xea9e..=0xea9f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5117449664429530,
            relative_height: 0.4051054384017758,
            relative_x: 0.2136465324384788,
            relative_y: 0.3068812430632630,
            ..Constraint::none
        }),
        0xeaa2 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.8061116965226555,
            relative_height: 0.9438247156716689,
            relative_x: 0.0679662802950474,
            relative_y: 0.0147523709167545,
            ..Constraint::none
        }),
        0xeab4 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.9945482866043613,
            relative_height: 0.5264797507788161,
            relative_y: 0.2024922118380062,
            ..Constraint::none
        }),
        0xeab5 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5264797507788161,
            relative_height: 0.9945482866043613,
            relative_x: 0.2024922118380062,
            relative_y: 0.0054517133956386,
            ..Constraint::none
        }),
        0xeab6 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5264797507788161,
            relative_height: 0.9945482866043613,
            relative_x: 0.2710280373831775,
            ..Constraint::none
        }),
        0xeab7 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.9945482866043613,
            relative_height: 0.5264797507788161,
            relative_x: 0.0054517133956386,
            relative_y: 0.2710280373831775,
            ..Constraint::none
        }),
        0xead4..=0xead5 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7069825436408977,
            relative_x: 0.1483790523690773,
            ..Constraint::none
        }),
        0xead6 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8780760626398211,
            relative_y: 0.0687919463087248,
            ..Constraint::none
        }),
        0xeb43 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7335766423357665,
            relative_height: 0.9996188152778837,
            relative_x: 0.1991657977059437,
            relative_y: 0.0003811847221163,
            ..Constraint::none
        }),
        0xeb6e | 0xeb71 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.4954604409857328,
            relative_y: 0.2522697795071336,
            ..Constraint::none
        }),
        0xeb6f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.4973958333333333,
            relative_x: 0.2493489583333333,
            ..Constraint::none
        }),
        0xeb70 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.4973958333333333,
            relative_height: 0.9961089494163424,
            relative_x: 0.2493489583333333,
            relative_y: 0.0038910505836576,
            ..Constraint::none
        }),
        0xeb8a => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.3468834688346883,
            relative_height: 0.3353615785256410,
            relative_x: 0.2642276422764228,
            relative_y: 0.3313050881410256,
            ..Constraint::none
        }),
        0xeb9a => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.8740779768177028,
            relative_height: 0.9438247156716689,
            relative_x: 0.0679662802950474,
            relative_y: 0.0147523709167545,
            ..Constraint::none
        }),
        0xebd5 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.9322210636079249,
            relative_height: 0.9318897917604415,
            relative_y: 0.0681102082395584,
            ..Constraint::none
        }),
        0xebd6 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9996446423917936,
            relative_y: 0.0003553576082064,
            ..Constraint::none
        }),
        0xec07 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.3495911047345768,
            relative_height: 0.3355179398148149,
            relative_x: 0.2615335565120357,
            relative_y: 0.3311487268518519,
            ..Constraint::none
        }),
        0xec0b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.9327424400417101,
            relative_height: 0.9996188152778837,
            relative_y: 0.0003811847221163,
            ..Constraint::none
        }),
        0xec0c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.8008342022940563,
            relative_x: 0.1991657977059437,
            ..Constraint::none
        }),
        0xee00 | 0xee03 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::End,
            align_vertical: Align::Center1,
            relative_width: 0.8681172291296625,
            relative_height: 0.8626692456479691,
            relative_x: 0.1314387211367673,
            relative_y: 0.0686653771760155,
            pad_left: -0.025,
            pad_right: -0.025,
            pad_top: -0.005,
            pad_bottom: -0.005,
            ..Constraint::none
        }),
        0xee01 | 0xee04 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8626692456479691,
            relative_y: 0.0686653771760155,
            pad_left: -0.05,
            pad_right: -0.05,
            pad_top: -0.005,
            pad_bottom: -0.005,
            ..Constraint::none
        }),
        0xee02 | 0xee05 => Some(Constraint {
            size: ConstraintSize::Stretch,
            max_constraint_width: 1,
            align_horizontal: Align::Start,
            align_vertical: Align::Center1,
            relative_width: 0.8685612788632326,
            relative_height: 0.8626692456479691,
            relative_y: 0.0686653771760155,
            pad_left: -0.025,
            pad_right: -0.025,
            pad_top: -0.005,
            pad_bottom: -0.005,
            ..Constraint::none
        }),
        0xee06 => Some(Constraint {
            size: ConstraintSize::Cover,
            max_constraint_width: 1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7059415911379657,
            relative_height: 0.2234524408656266,
            relative_x: 0.1470292044310171,
            relative_y: 0.7765475591343735,
            pad_left: 0.015,
            pad_right: 0.015,
            pad_top: 0.015,
            pad_bottom: 0.015,
            ..Constraint::none
        }),
        0xee07 => Some(Constraint {
            size: ConstraintSize::Cover,
            max_constraint_width: 1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5000000000000000,
            relative_height: 0.7498741821841973,
            relative_x: 0.5000000000000000,
            relative_y: 0.2501258178158027,
            pad_left: 0.015,
            pad_right: 0.015,
            pad_top: 0.015,
            pad_bottom: 0.015,
            ..Constraint::none
        }),
        0xee08 => Some(Constraint {
            size: ConstraintSize::Cover,
            max_constraint_width: 1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6299093655589124,
            relative_height: 0.8535480624056366,
            relative_x: 0.3700906344410876,
            pad_left: 0.015,
            pad_right: 0.015,
            pad_top: 0.015,
            pad_bottom: 0.015,
            ..Constraint::none
        }),
        0xee09 => Some(Constraint {
            size: ConstraintSize::Cover,
            max_constraint_width: 1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.4997483643683945,
            pad_left: 0.015,
            pad_right: 0.015,
            pad_top: 0.015,
            pad_bottom: 0.015,
            ..Constraint::none
        }),
        0xee0a => Some(Constraint {
            size: ConstraintSize::Cover,
            max_constraint_width: 1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6299093655589124,
            relative_height: 0.8535480624056366,
            pad_left: 0.015,
            pad_right: 0.015,
            pad_top: 0.015,
            pad_bottom: 0.015,
            ..Constraint::none
        }),
        0xee0b => Some(Constraint {
            size: ConstraintSize::Cover,
            max_constraint_width: 1,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5000000000000000,
            relative_height: 0.7498741821841973,
            relative_y: 0.2501258178158027,
            pad_left: 0.015,
            pad_right: 0.015,
            pad_top: 0.015,
            pad_bottom: 0.015,
            ..Constraint::none
        }),
        0xf005 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9999664113932554,
            relative_y: 0.0000335886067446,
            ..Constraint::none
        }),
        0xf026..=0xf027 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9786184354605580,
            relative_y: 0.0103951316192896,
            ..Constraint::none
        }),
        0xf02b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9758052740827267,
            relative_y: 0.0238869355863696,
            ..Constraint::none
        }),
        0xf031..=0xf033 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9987922705314010,
            relative_y: 0.0006038647342995,
            ..Constraint::none
        }),
        0xf035 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9989935587761675,
            relative_y: 0.0004025764895330,
            ..Constraint::none
        }),
        0xf044 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9925925925925926,
            ..Constraint::none
        }),
        0xf046 | 0xf153..=0xf154 | 0xf158 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8751322751322751,
            relative_y: 0.0624338624338624,
            ..Constraint::none
        }),
        0xf048 | 0xf04a | 0xf04e | 0xf051 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8577706898990622,
            relative_y: 0.0711892586341537,
            ..Constraint::none
        }),
        0xf049 | 0xf050 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8579450878868969,
            relative_y: 0.0710148606463189,
            ..Constraint::none
        }),
        0xf04b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9997041418532618,
            relative_y: 0.0002958581467381,
            ..Constraint::none
        }),
        0xf04c..=0xf04d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8572940020656472,
            relative_y: 0.0713404035569438,
            ..Constraint::none
        }),
        0xf04f => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7138835298072554,
            relative_y: 0.1433479295317200,
            ..Constraint::none
        }),
        0xf052 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9999748091795350,
            ..Constraint::none
        }),
        0xf060..=0xf061 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8567975830815709,
            relative_y: 0.0719033232628399,
            ..Constraint::none
        }),
        0xf063 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9987915407854985,
            relative_y: 0.0006042296072508,
            ..Constraint::none
        }),
        0xf077 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5700483091787439,
            relative_y: 0.2862318840579710,
            ..Constraint::none
        }),
        0xf078 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5700483091787439,
            relative_y: 0.1437198067632850,
            ..Constraint::none
        }),
        0xf07e => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.4989429175475687,
            relative_y: 0.2505285412262157,
            ..Constraint::none
        }),
        0xf089 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9998488512696494,
            relative_y: 0.0001511487303507,
            ..Constraint::none
        }),
        0xf0a4..=0xf0a5 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7502645502645503,
            relative_y: 0.1248677248677249,
            ..Constraint::none
        }),
        0xf0d7 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.4281400966183575,
            relative_y: 0.2053140096618357,
            ..Constraint::none
        }),
        0xf0d8 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.4281400966183575,
            relative_y: 0.3472222222222222,
            ..Constraint::none
        }),
        0xf0d9 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7140772371750631,
            relative_y: 0.1333462732919255,
            ..Constraint::none
        }),
        0xf0da => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7140396210163651,
            relative_y: 0.1333838894506235,
            ..Constraint::none
        }),
        0xf0dc => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            ..Constraint::none
        }),
        0xf0dd => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            relative_height: 0.4275362318840580,
            relative_y: 0.0012077294685990,
            ..Constraint::none
        }),
        0xf0de => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            relative_height: 0.4287439613526570,
            relative_y: 0.5712560386473430,
            ..Constraint::none
        }),
        0xf100..=0xf101 | 0xf104..=0xf105 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8573155985489722,
            relative_y: 0.0713422007255139,
            ..Constraint::none
        }),
        0xf102 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9286577992744861,
            relative_y: 0.0713422007255139,
            ..Constraint::none
        }),
        0xf103 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9286577992744861,
            ..Constraint::none
        }),
        0xf106..=0xf107 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.5000000000000000,
            relative_y: 0.2853688029020556,
            ..Constraint::none
        }),
        0xf130 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9998602571268865,
            ..Constraint::none
        }),
        0xf141 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.2593984962406015,
            relative_y: 0.3696741854636592,
            ..Constraint::none
        }),
        0xf156 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8752505446623093,
            relative_y: 0.0623155929038282,
            ..Constraint::none
        }),
        0xf157 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8756468797564688,
            relative_y: 0.0624338624338624,
            ..Constraint::none
        }),
        0xf159 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8756067947646895,
            relative_y: 0.0623492063492063,
            ..Constraint::none
        }),
        0xf175 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9989423585404548,
            relative_y: 0.0005288207297726,
            ..Constraint::none
        }),
        0xf177..=0xf178 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6250661025912215,
            relative_y: 0.1877313590692755,
            ..Constraint::none
        }),
        0xf182 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.9998046921689268,
            ..Constraint::none
        }),
        0xf221 | 0xf224..=0xf226 | 0xf228 | 0xf22a | 0xf22c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9994854643684076,
            ..Constraint::none
        }),
        0xf222 | 0xf227 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.8746819883943630,
            relative_y: 0.0624017379870223,
            ..Constraint::none
        }),
        0xf229 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9370837263813853,
            relative_y: 0.0624017379870223,
            ..Constraint::none
        }),
        0xf22b | 0xf22d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6874767744332962,
            relative_y: 0.1560043449675557,
            ..Constraint::none
        }),
        0xf255..=0xf256 | 0xf25a => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9993997599039616,
            ..Constraint::none
        }),
        0xf257 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7810124049619848,
            relative_y: 0.0935945806894186,
            ..Constraint::none
        }),
        0xf258 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.7498142113988452,
            relative_y: 0.1247927742525582,
            ..Constraint::none
        }),
        0xf25b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.9975006099019084,
            ..Constraint::none
        }),
        0xf416 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_height: 0.6090604026845637,
            relative_y: 0.2119686800894855,
            ..Constraint::none
        }),
        0xf424 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5019531250000000,
            relative_height: 0.5755033557046980,
            relative_x: 0.2480468750000000,
            relative_y: 0.2108501118568233,
            ..Constraint::none
        }),
        0xf431 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6240234375000000,
            relative_height: 0.7695749440715883,
            relative_x: 0.2031250000000000,
            relative_y: 0.1420581655480984,
            ..Constraint::none
        }),
        0xf432 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6718750000000000,
            relative_height: 0.7147651006711410,
            relative_x: 0.1875000000000000,
            relative_y: 0.1610738255033557,
            ..Constraint::none
        }),
        0xf433 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6240234375000000,
            relative_height: 0.7695749440715883,
            relative_x: 0.2041015625000000,
            relative_y: 0.0883668903803132,
            ..Constraint::none
        }),
        0xf434 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6718750000000000,
            relative_height: 0.7147651006711410,
            relative_x: 0.1406250000000000,
            relative_y: 0.1599552572706935,
            ..Constraint::none
        }),
        0xf438 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.2436523437500000,
            relative_height: 0.4560546875000000,
            relative_x: 0.3813476562500000,
            relative_y: 0.2719726562500000,
            ..Constraint::none
        }),
        0xf43e => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5029296875000000,
            relative_height: 0.5755033557046980,
            relative_x: 0.2500000000000000,
            relative_y: 0.2136465324384788,
            ..Constraint::none
        }),
        0xf443 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7500000000000000,
            relative_x: 0.1250000000000000,
            ..Constraint::none
        }),
        0xf444..=0xf445 | 0xf4c3 | 0xf51d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5000000000000000,
            relative_height: 0.5000000000000000,
            relative_x: 0.2500000000000000,
            relative_y: 0.2500000000000000,
            ..Constraint::none
        }),
        0xf44a => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.2436523437500000,
            relative_height: 0.4560546875000000,
            relative_x: 0.3750000000000000,
            relative_y: 0.2719726562500000,
            ..Constraint::none
        }),
        0xf44b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.4560546875000000,
            relative_height: 0.2436523437500000,
            relative_x: 0.2719726562500000,
            relative_y: 0.3188476562500000,
            ..Constraint::none
        }),
        0xf45c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5019531250000000,
            relative_height: 0.5749440715883669,
            relative_x: 0.2480468750000000,
            relative_y: 0.2114093959731544,
            ..Constraint::none
        }),
        0xf460 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.3593750000000000,
            relative_height: 0.6240234375000000,
            relative_x: 0.3750000000000000,
            relative_y: 0.1884765625000000,
            ..Constraint::none
        }),
        0xf461 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6237816764132553,
            relative_height: 0.9988851727982163,
            relative_x: 0.1881091617933723,
            ..Constraint::none
        }),
        0xf467 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5639648437500000,
            relative_height: 0.5649414062500000,
            relative_x: 0.2187500000000000,
            relative_y: 0.2177734375000000,
            ..Constraint::none
        }),
        0xf46c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5039062500000000,
            relative_height: 0.5771812080536913,
            relative_x: 0.2490234375000000,
            relative_y: 0.2091722595078300,
            ..Constraint::none
        }),
        0xf470 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.9926757812500000,
            relative_height: 0.2690429687500000,
            relative_y: 0.6865234375000000,
            ..Constraint::none
        }),
        0xf476 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.8732325694783033,
            relative_x: 0.0633837152608484,
            ..Constraint::none
        }),
        0xf47a => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.5843079922027290,
            relative_height: 0.9509476031215162,
            relative_x: 0.2066276803118908,
            relative_y: 0.0234113712374582,
            ..Constraint::none
        }),
        0xf47b..=0xf47c => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.6250000000000000,
            relative_height: 0.3593750000000000,
            relative_x: 0.1875000000000000,
            relative_y: 0.3281250000000000,
            ..Constraint::none
        }),
        0xf47d => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.3593750000000000,
            relative_height: 0.6240234375000000,
            relative_x: 0.2656250000000000,
            relative_y: 0.1875000000000000,
            ..Constraint::none
        }),
        0xf47e => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.4560546875000000,
            relative_height: 0.2436523437500000,
            relative_x: 0.2719726562500000,
            relative_y: 0.3750000000000000,
            ..Constraint::none
        }),
        0xf48b => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7187500000000000,
            relative_height: 0.0937500000000000,
            relative_x: 0.1250000000000000,
            relative_y: 0.4687500000000000,
            ..Constraint::none
        }),
        0xf493 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.8313840155945419,
            relative_height: 0.9509476031215162,
            relative_x: 0.0843079922027290,
            relative_y: 0.0234113712374582,
            ..Constraint::none
        }),
        0xf49a => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.8727450024378351,
            relative_x: 0.0633837152608484,
            ..Constraint::none
        }),
        0xf4ef | 0xf4f2 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.7142857142857143,
            relative_x: 0.1428571428571428,
            ..Constraint::none
        }),
        0xf4f0 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.9642857142857143,
            relative_height: 0.7407407407407407,
            relative_y: 0.1111111111111111,
            ..Constraint::none
        }),
        0xf4f1 => Some(Constraint {
            size: ConstraintSize::FitCover1,
            height: Height::Icon,
            align_horizontal: Align::Center1,
            align_vertical: Align::Center1,
            relative_width: 0.9642857142857143,
            relative_height: 0.7407407407407407,
            relative_x: 0.0357142857142857,
            relative_y: 0.1111111111111111,
            ..Constraint::none
        }),
        _ => None,
    }
}
