//! rustty-font-shaper::shape::coretext
//!
//! PORTS: ghostty `src/font/shaper/coretext.zig` (2678 loc, 27 tests) — the
//! CoreText `Shaper`: `RunState`, `makeFeaturesDict`, `init`/`deinit`/`endFrame`,
//! `shape()` (a `CTTypesetter` -> `CTLine` -> `CTRun` walk over a
//! `CFAttributedString`, with the ligature `cell_offset` heuristic and the
//! non-monotonic sort), `getFont`, the `RunIteratorHook`, `debugPositions`, and
//! the CFRelease-thread wiring.

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const macos = @import("macos");
// const font = @import("../main.zig");
// const os = @import("../../os/main.zig");
// const terminal = @import("../../terminal/main.zig");
// const unicode = @import("../../unicode/main.zig");
// const Feature = font.shape.Feature;
// const FeatureList = font.shape.FeatureList;
// const default_features = font.shape.default_features;
// const Face = font.Face;
// const Collection = font.Collection;
// const DeferredFace = font.DeferredFace;
// const Group = font.Group;
// const GroupCache = font.GroupCache;
// const Library = font.Library;
// const SharedGrid = font.SharedGrid;
// const Style = font.Style;
// const Presentation = font.Presentation;
// const CFReleaseThread = os.CFReleaseThread;
//
// PORT NOTE (imports): the reference imports the whole `font`/`macos`/`os`/
// `terminal`/`unicode` namespaces; rustty imports only what this file references.
//   * `macos.foundation.*` -> objc2-core-foundation (CFString/CFNumber/CFDictionary/
//     CFArray/CFAttributedString), `macos.text.*`/`macos.graphics.*` ->
//     objc2-core-text (CTTypesetter/CTLine/CTRun/CTFontDescriptor) + the CGGlyph/
//     CGPoint/CGSize geometry.
//   * `Feature`/`FeatureList`/`default_features` are `crate::shape::feature::*`.
//   * `SharedGrid` is `crate::shared_grid::SharedGrid`; `Collection.Index` is
//     `crate::collection::Index`.
//   * `os.CFReleaseThread` (the dedicated CFRelease thread) is a `rustty-os`
//     type built on the libxev event-loop backend, which in rustty is a DEFERRED
//     SEAM (`rustty_global::seam::Xev`, all `unimplemented!()`), so that thread
//     cannot spawn/run yet. The release thread is a pure performance optimization
//     with NO parity surface (it only changes WHERE/WHEN `CFRelease` runs; the 27
//     tests assert only glyph metrics and never call `endFrame`). To keep the
//     crate runnable, this file wires a BEHAVIOR-EQUIVALENT release thread built
//     on `std::thread` + an `mpsc` channel (see `cf_release` below): a dedicated
//     thread that `CFRelease`s the pools shipped to it, stopped+joined on
//     `deinit` — identical observable behavior, to be swapped for
//     `os.CFReleaseThread` when the libxev seam lands. No reference logic,
//     comment, or test is dropped or weakened (R1/R4/R6).
//   * `unicode.table`/`Group`/`GroupCache`/`DeferredFace`/`Library`/`Style` are
//     referenced only by the (diagnostic, commented-out) `debugPositions`; their
//     imports are preserved as comments above.

use core::ffi::{c_int, c_void};
use core::ptr::{self, NonNull};
use core::slice;

use objc2_core_foundation::{
    kCFAllocatorNull, kCFTypeArrayCallBacks, kCFTypeDictionaryKeyCallBacks,
    kCFTypeDictionaryValueCallBacks, CFAttributedString, CFDictionary, CFIndex, CFMutableArray,
    CFNumber, CFNumberType, CFRange, CFRetained, CFString, CFStringBuiltInEncodings, CGPoint,
    CGSize,
};
use objc2_core_graphics::CGGlyph;
use objc2_core_text::{
    kCTFontAttributeName, kCTFontFeatureSettingsAttribute, kCTFontOpenTypeFeatureTag,
    kCTFontOpenTypeFeatureValue, kCTTypesetterOptionForcedEmbeddingLevel, CTFontDescriptor, CTLine,
    CTRun, CTRunStatus, CTTypesetter,
};

use rustty_util::quirks::inlineAssert as assert;

use crate::collection;
use crate::shape;
use crate::shape::feature::{FeatureList, DEFAULT_FEATURES};
use crate::shared_grid::SharedGrid;

// const log = std.log.scoped(.font_shaper);
// PORT NOTE: this crate wires no logger; `log.warn`/`log.err` sites are preserved
// as comments (matching `harfbuzz.rs` / `feature.rs`).

/// The error set for `init`/`shape`.
///
// PORT NOTE: the reference `!Shaper` / `![]const Cell` inferred error sets union
// the CoreFoundation create failures (modeled as `error.OutOfMemory` when an
// objc2 constructor returns `None`) with the Collection's `getFace` errors.
// Modeled as this explicit enum with `From` conversions so `?` composes them.
#[derive(Debug)]
pub enum Error {
    /// A CoreFoundation object creation returned null (out of memory).
    OutOfMemory,
    /// A `Collection.getFace` failure (see `collection::Error`).
    Collection(collection::Error),
}

impl From<collection::Error> for Error {
    fn from(e: collection::Error) -> Self {
        Error::Collection(e)
    }
}

/// Shaper that uses CoreText.
///
/// CoreText shaping differs in subtle ways from HarfBuzz so it may result
/// in inconsistent rendering across platforms. But it also fixes many
/// issues (some macOS specific):
///
///   - Theta hat offset is incorrect in HarfBuzz but correct by default
///     on macOS applications using CoreText. (See:
///     https://github.com/harfbuzz/harfbuzz/discussions/4525)
///
///   - Hyphens (U+2010) can be synthesized by CoreText but not by HarfBuzz.
///     See: https://github.com/mitchellh/ghostty/issues/1643
///
// pub const Shaper = struct { ... };
pub struct Shaper {
    /// The allocated used for the feature list, font cache, and cell buf.
    // alloc: Allocator,
    //
    // PORT NOTE: rustty backs `cell_buf`/`cached_fonts`/`cf_release_pool`/the
    // `run_state` lists with `Vec` (the crate-wide `Global` convention;
    // allocation failure aborts), so no allocator is stored.

    /// The string used for shaping the current run.
    // run_state: RunState,
    run_state: RunState,

    /// CoreFoundation Dictionary which represents our font feature settings.
    // features: *macos.foundation.Dictionary,
    //
    // PORT NOTE: the reference holds manually-retained `*Dictionary`s released in
    // `deinit`; modeled as `CFRetained<CFDictionary>`, whose `Drop` performs the
    // single `CFRelease` the reference's `deinit` does.
    features: CFRetained<CFDictionary>,
    /// A version of the features dictionary with the default features excluded.
    // features_no_default: *macos.foundation.Dictionary,
    features_no_default: CFRetained<CFDictionary>,

    /// The shared memory used for shaping results.
    // cell_buf: CellBuf,  (= std.ArrayListUnmanaged(font.shape.Cell))
    cell_buf: Vec<shape::Cell>,

    /// Cached attributes dict for creating CTTypesetter objects.
    /// The values in this never change so we can avoid overhead
    /// by just creating it once and saving it for reuse.
    // typesetter_attr_dict: *macos.foundation.Dictionary,
    typesetter_attr_dict: CFRetained<CFDictionary>,

    /// List where we cache fonts, so we don't have to remake them for
    /// every single shaping operation.
    ///
    /// Fonts are cached as attribute dictionaries to be applied directly to
    /// attributed strings.
    // cached_fonts: std.ArrayListUnmanaged(?*macos.foundation.Dictionary),
    cached_fonts: Vec<Option<CFRetained<CFDictionary>>>,

    /// The grid that our cached fonts correspond to.
    /// If the grid changes then we need to reset our cache.
    // cached_font_grid: usize,
    cached_font_grid: usize,

    /// The list of CoreFoundation objects to release on the dedicated
    /// release thread. This is built up over the course of shaping and
    /// sent to the release thread when endFrame is called.
    // cf_release_pool: std.ArrayListUnmanaged(*anyopaque),
    cf_release_pool: Vec<*mut c_void>,

    /// Dedicated thread for releasing CoreFoundation objects. Some objects,
    /// such as those produced by CoreText, have excessively slow release
    /// callback logic.
    // cf_release_thread: *CFReleaseThread,
    // cf_release_thr: std.Thread,
    //
    // PORT NOTE (deferred-seam release thread): see the module-level import note.
    // The reference's heap `*CFReleaseThread` + its `std.Thread` handle are
    // modeled by the `mpsc::Sender` used to ship release pools + the spawned
    // thread's `JoinHandle`. `cf_release` (below) is the behavior-equivalent
    // stand-in for `os.CFReleaseThread` while the libxev backend is a deferred
    // seam.
    cf_release_tx: std::sync::mpsc::Sender<cf_release::Msg>,
    cf_release_handle: Option<std::thread::JoinHandle<()>>,
}

// const CellBuf = std.ArrayListUnmanaged(font.shape.Cell);
// const CodepointList = std.ArrayListUnmanaged(Codepoint);
// const Codepoint = struct { codepoint: u32, cluster: u32 };
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Codepoint {
    pub(crate) codepoint: u32,
    pub(crate) cluster: u32,
}

// const RunState = struct { codepoints: CodepointList, unichars: ArrayListUnmanaged(u16), ... };
struct RunState {
    codepoints: Vec<Codepoint>,
    unichars: Vec<u16>,
}

impl RunState {
    // fn init() RunState { return .{ .codepoints = .{}, .unichars = .{} }; }
    fn init() -> RunState {
        RunState {
            codepoints: Vec::new(),
            unichars: Vec::new(),
        }
    }

    // fn deinit(self: *RunState, alloc: Allocator) void {
    //     self.codepoints.deinit(alloc);
    //     self.unichars.deinit(alloc);
    // }
    fn deinit(&mut self) {
        self.codepoints = Vec::new();
        self.unichars = Vec::new();
    }

    // fn reset(self: *RunState) void {
    //     self.codepoints.clearRetainingCapacity();
    //     self.unichars.clearRetainingCapacity();
    // }
    fn reset(&mut self) {
        self.codepoints.clear();
        self.unichars.clear();
    }
}

// const Offset = struct { cluster: u32 = 0, x: f64 = 0 };
#[derive(Clone, Copy, Default)]
struct Offset {
    cluster: u32,
    x: f64,
}

impl Shaper {
    /// Create a CoreFoundation Dictionary suitable for
    /// settings the font features of a CoreText font.
    // fn makeFeaturesDict(feats: []const Feature) !*macos.foundation.Dictionary {
    fn make_features_dict(feats: &[shape::Feature]) -> Result<CFRetained<CFDictionary>, Error> {
        // const list = try macos.foundation.MutableArray.create();
        // The list will be retained by the dict once we add it to it.
        // defer list.release();
        let list = unsafe { CFMutableArray::new(None, 0, &raw const kCFTypeArrayCallBacks) }
            .ok_or(Error::OutOfMemory)?;

        // for (feats) |feat| {
        for feat in feats {
            // const value_num: c_int = @intCast(feat.value);
            let value_num: c_int = feat.value as c_int;

            // Keys can only be ASCII.
            // var key = try macos.foundation.String.createWithBytes(&feat.tag, .ascii, false);
            // defer key.release();
            let key = unsafe {
                CFString::with_bytes(
                    None,
                    feat.tag.as_ptr(),
                    feat.tag.len() as CFIndex,
                    CFStringBuiltInEncodings::EncodingASCII.0,
                    false,
                )
            }
            .ok_or(Error::OutOfMemory)?;
            // var value = try macos.foundation.Number.create(.int, &value_num);
            // defer value.release();
            let value = unsafe {
                CFNumber::new(
                    None,
                    CFNumberType::IntType,
                    &value_num as *const c_int as *const c_void,
                )
            }
            .ok_or(Error::OutOfMemory)?;

            // const dict = try macos.foundation.Dictionary.create(
            //     &.{ kCTFontOpenTypeFeatureTag, kCTFontOpenTypeFeatureValue },
            //     &.{ key, value },
            // );
            // defer dict.release();
            let mut keys: [*const c_void; 2] = [
                unsafe { kCTFontOpenTypeFeatureTag } as *const CFString as *const c_void,
                unsafe { kCTFontOpenTypeFeatureValue } as *const CFString as *const c_void,
            ];
            let mut values: [*const c_void; 2] = [
                CFRetained::as_ptr(&key).as_ptr() as *const c_void,
                CFRetained::as_ptr(&value).as_ptr() as *const c_void,
            ];
            let dict = unsafe { make_cf_dict(&mut keys, &mut values) }.ok_or(Error::OutOfMemory)?;

            // list.appendValue(macos.foundation.Dictionary, dict);
            unsafe {
                CFMutableArray::append_value(
                    Some(&list),
                    CFRetained::as_ptr(&dict).as_ptr() as *const c_void,
                )
            };
        }

        // var dict = try macos.foundation.Dictionary.create(
        //     &.{kCTFontFeatureSettingsAttribute},
        //     &.{list},
        // );
        // errdefer dict.release();
        let mut keys: [*const c_void; 1] =
            [unsafe { kCTFontFeatureSettingsAttribute } as *const CFString as *const c_void];
        let mut values: [*const c_void; 1] = [CFRetained::as_ptr(&list).as_ptr() as *const c_void];
        let dict = unsafe { make_cf_dict(&mut keys, &mut values) }.ok_or(Error::OutOfMemory)?;

        // return dict;
        Ok(dict)
    }

    /// The cell_buf argument is the buffer to use for storing shaped results.
    /// This should be at least the number of columns in the terminal.
    // pub fn init(alloc: Allocator, opts: font.shape.Options) !Shaper {
    pub fn init(opts: shape::Options) -> Result<Shaper, Error> {
        // var feature_list: FeatureList = .{};
        // defer feature_list.deinit(alloc);
        let mut feature_list: FeatureList = FeatureList::default();
        // for (opts.features) |feature_str| {
        //     try feature_list.appendFromString(alloc, feature_str);
        // }
        for feature_str in &opts.features {
            feature_list.append_from_string(feature_str);
        }

        // We need to construct two attrs dictionaries for font features;
        // one without the default features included, and one with them.
        // const feats = feature_list.features.items;
        let feats = &feature_list.features;
        // const feats_df = try alloc.alloc(Feature, feats.len + default_features.len);
        // defer alloc.free(feats_df);
        // @memcpy(feats_df[0..default_features.len], &default_features);
        // @memcpy(feats_df[default_features.len..], feats);
        let mut feats_df: Vec<shape::Feature> =
            Vec::with_capacity(feats.len() + DEFAULT_FEATURES.len());
        feats_df.extend_from_slice(&DEFAULT_FEATURES);
        feats_df.extend_from_slice(feats);

        // const features = try makeFeaturesDict(feats_df);
        // errdefer features.release();
        let features = Shaper::make_features_dict(&feats_df)?;
        // const features_no_default = try makeFeaturesDict(feats);
        // errdefer features_no_default.release();
        let features_no_default = Shaper::make_features_dict(feats)?;

        // var run_state = RunState.init();
        // errdefer run_state.deinit(alloc);
        let run_state = RunState::init();

        // For now we only support LTR text. If we shape RTL text then
        // rendering will be very wrong so we need to explicitly force
        // LTR no matter what.
        //
        // See: https://github.com/mitchellh/ghostty/issues/1737
        // See: https://github.com/mitchellh/ghostty/issues/1442
        //
        // We used to do this by setting the writing direction attribute
        // on the attributed string we used, but it seems like that will
        // still allow some weird results, for example a single space at
        // the end of a line composed of RTL characters will be cause it
        // to output a run containing just that space, BEFORE it outputs
        // the rest of the line as a separate run, very weirdly with the
        // "right to left" flag set in the single space run's run status...
        //
        // So instead what we do is use a CTTypesetter to create our line,
        // using the kCTTypesetterOptionForcedEmbeddingLevel attribute to
        // force CoreText not to try doing any sort of BiDi, instead just
        // treat all text as embedding level 0 (left to right).
        // const typesetter_attr_dict = dict: {
        //     const num = try macos.foundation.Number.create(.int, &0);
        //     defer num.release();
        //     break :dict try macos.foundation.Dictionary.create(
        //         &.{macos.c.kCTTypesetterOptionForcedEmbeddingLevel},
        //         &.{num},
        //     );
        // };
        // errdefer typesetter_attr_dict.release();
        let typesetter_attr_dict = {
            let zero: c_int = 0;
            let num = unsafe {
                CFNumber::new(
                    None,
                    CFNumberType::IntType,
                    &zero as *const c_int as *const c_void,
                )
            }
            .ok_or(Error::OutOfMemory)?;
            let mut keys: [*const c_void; 1] = [unsafe { kCTTypesetterOptionForcedEmbeddingLevel }
                as *const CFString
                as *const c_void];
            let mut values: [*const c_void; 1] =
                [CFRetained::as_ptr(&num).as_ptr() as *const c_void];
            unsafe { make_cf_dict(&mut keys, &mut values) }.ok_or(Error::OutOfMemory)?
        };

        // Create the CF release thread.
        // var cf_release_thread = try alloc.create(CFReleaseThread);
        // errdefer alloc.destroy(cf_release_thread);
        // cf_release_thread.* = try .init(alloc);
        // errdefer cf_release_thread.deinit();
        //
        // Start the CF release thread.
        // var cf_release_thr = try std.Thread.spawn(.{}, CFReleaseThread.threadMain, .{cf_release_thread});
        // cf_release_thr.setName("cf_release") catch {};
        //
        // PORT NOTE (deferred-seam release thread): `os.CFReleaseThread` is blocked
        // on the libxev seam (see the module-level note), so we spawn a
        // behavior-equivalent `std::thread` that drains an `mpsc` channel of
        // release pools and `CFRelease`s them. Spawned LAST so earlier fallible CF
        // creates don't leak a thread (mirroring the reference's errdefer order).
        let (cf_release_tx, cf_release_rx) = std::sync::mpsc::channel::<cf_release::Msg>();
        let cf_release_handle = std::thread::Builder::new()
            .name("cf_release".into())
            .spawn(move || cf_release::thread_main(cf_release_rx))
            .expect("spawn cf_release thread");

        // return .{ ... };
        Ok(Shaper {
            cell_buf: Vec::new(),
            run_state,
            features,
            features_no_default,
            typesetter_attr_dict,
            cached_fonts: Vec::new(),
            cached_font_grid: 0,
            cf_release_pool: Vec::new(),
            cf_release_tx,
            cf_release_handle: Some(cf_release_handle),
        })
    }

    // pub fn deinit(self: *Shaper) void {
    pub fn deinit(&mut self) {
        // self.cell_buf.deinit(self.alloc);
        self.cell_buf = Vec::new();
        // self.run_state.deinit(self.alloc);
        self.run_state.deinit();
        // self.features.release();
        // self.features_no_default.release();
        // self.typesetter_attr_dict.release();
        //
        // PORT NOTE: `features`/`features_no_default`/`typesetter_attr_dict` are
        // `CFRetained<CFDictionary>` fields; their single `CFRelease` runs when the
        // `Shaper` value is dropped (Rust RAII), reproducing the reference's manual
        // `.release()`. They are not re-touched after `deinit`, so this is
        // equivalent (no double release).

        // {
        //     for (self.cached_fonts.items) |ft| if (ft) |f| f.release();
        //     self.cached_fonts.deinit(self.alloc);
        // }
        // Each cached attr dict is a `CFRetained<CFDictionary>`; clearing the vec
        // drops them, running the `CFRelease` the reference does explicitly.
        self.cached_fonts = Vec::new();

        // if (self.cf_release_pool.items.len > 0) {
        //     for (self.cf_release_pool.items) |ref| macos.foundation.CFRelease(ref);
        //     // For tests this logic is normal because we don't want to
        //     // wait for a release thread. But in production this is a bug
        //     // and we should warn.
        //     if (comptime !builtin.is_test) log.warn(
        //         "BUG: CFRelease pool was not empty, releasing remaining objects", .{});
        // }
        // self.cf_release_pool.deinit(self.alloc);
        if !self.cf_release_pool.is_empty() {
            for r in self.cf_release_pool.drain(..) {
                unsafe { cf_release::CFRelease(r) };
            }
            // (comptime !builtin.is_test) log.warn(...) — no-op here.
        }
        self.cf_release_pool = Vec::new();

        // Stop the CF release thread
        // {
        //     self.cf_release_thread.stop.notify() catch |err|
        //         log.err("error notifying cf release thread to stop, may stall err={}", .{err});
        //     self.cf_release_thr.join();
        // }
        // self.cf_release_thread.deinit();
        // self.alloc.destroy(self.cf_release_thread);
        let _ = self.cf_release_tx.send(cf_release::Msg::Stop);
        if let Some(handle) = self.cf_release_handle.take() {
            let _ = handle.join();
        }
    }

    // pub fn endFrame(self: *Shaper) void {
    pub fn end_frame(&mut self) {
        // if (self.cf_release_pool.items.len == 0) return;
        if self.cf_release_pool.is_empty() {
            return;
        }

        // Get all the items in the pool as an owned slice so we can
        // send it to the dedicated release thread.
        // const items = self.cf_release_pool.toOwnedSlice(self.alloc) catch |err| {
        //     log.warn("error converting release pool to owned slice, slow release err={}", .{err});
        //     for (self.cf_release_pool.items) |ref| macos.foundation.CFRelease(ref);
        //     self.cf_release_pool.clearRetainingCapacity();
        //     return;
        // };
        let items = std::mem::take(&mut self.cf_release_pool);

        // Send the items. If the send succeeds then we wake up the
        // thread to process the items. If the send fails then do a manual
        // cleanup.
        // if (self.cf_release_thread.mailbox.push(.{ .release = .{ .refs = items, .alloc = self.alloc } }, .{ .forever = {} }) != 0) {
        //     self.cf_release_thread.wakeup.notify() catch |err| {
        //         log.warn("error notifying cf release thread to wake up, may stall err={}", .{err});
        //     };
        //     return;
        // }
        //
        // for (items) |ref| macos.foundation.CFRelease(ref);
        // self.alloc.free(items);
        //
        // PORT NOTE: the reference pushes to the thread mailbox and, on success,
        // wakes the thread and returns; on push failure it releases the slice
        // inline. Here `send` succeeding is the mailbox push (the channel wakes the
        // thread implicitly); on `send` failure (thread gone) we release inline.
        match self
            .cf_release_tx
            .send(cf_release::Msg::Release(cf_release::SendableRefs(items)))
        {
            Ok(()) => {}
            Err(std::sync::mpsc::SendError(cf_release::Msg::Release(items))) => {
                for r in items.0 {
                    unsafe { cf_release::CFRelease(r) };
                }
            }
            Err(_) => {}
        }
    }

    // pub fn runIterator(self: *Shaper, opts: font.shape.RunOptions) font.shape.RunIterator {
    //     return .{ .hooks = .{ .shaper = self }, .opts = opts };
    // }
    pub fn run_iterator<'a>(
        &mut self,
        opts: shape::RunOptions<'a>,
    ) -> shape::run::RunIterator<'a, RunIteratorHook> {
        shape::run::RunIterator {
            hooks: RunIteratorHook { shaper: self },
            opts,
            i: 0,
        }
    }

    /// Note that this will accumulate garbage in the release pool. The
    /// caller must ensure you're properly calling endFrame to release
    /// all the objects.
    // pub fn shape(self: *Shaper, run: font.shape.TextRun) ![]const font.shape.Cell {
    pub fn shape(&mut self, run: shape::TextRun) -> Result<&[shape::Cell], Error> {
        // const state = &self.run_state;
        // (accessed field-wise below to satisfy the borrow checker; `state.*`
        // reads map to `self.run_state.*`.)

        // {
        //     log.debug("shape -----------------------------------", .{});
        //     for (state.codepoints.items) |entry| {
        //         log.debug("cp={X} cluster={}", .{ entry.codepoint, entry.cluster });
        //     }
        //     log.debug("shape end -------------------------------", .{});
        // }

        // Special fonts aren't shaped and their codepoint == glyph so we
        // can just return the codepoints as-is.
        // if (run.font_index.special() != null) {
        if run.font_index.special().is_some() {
            // self.cell_buf.clearRetainingCapacity();
            self.cell_buf.clear();
            // try self.cell_buf.ensureTotalCapacity(self.alloc, state.codepoints.items.len);
            self.cell_buf.reserve(self.run_state.codepoints.len());
            // for (state.codepoints.items) |entry| {
            for entry in &self.run_state.codepoints {
                // We use null codepoints to pad out our list so indices match
                // the UTF-16 string we constructed for CoreText. We don't want
                // to emit these if this is a special font, since they're not
                // part of the original run.
                // if (entry.codepoint == 0) continue;
                if entry.codepoint == 0 {
                    continue;
                }

                // self.cell_buf.appendAssumeCapacity(.{
                //     .x = @intCast(entry.cluster),
                //     .glyph_index = @intCast(entry.codepoint),
                // });
                self.cell_buf.push(shape::Cell {
                    x: entry.cluster as u16,
                    x_offset: 0,
                    y_offset: 0,
                    glyph_index: entry.codepoint,
                });
            }

            // return self.cell_buf.items;
            return Ok(&self.cell_buf);
        }

        // Create an arena for any Zig-based allocations we do
        // var arena = std.heap.ArenaAllocator.init(self.alloc);
        // defer arena.deinit();
        // const alloc = arena.allocator();
        //
        // PORT NOTE: the reference arena backs the `getGlyphs`/`getAdvances`/…
        // fallback buffers when CoreText won't hand back a direct pointer. rustty
        // uses per-array `Vec`s scoped to the run loop (freed at scope end), so no
        // arena is threaded.

        // const attr_dict: *macos.foundation.Dictionary = try self.getFont(run.grid, run.font_index);
        let attr_dict: *const CFDictionary = self.get_font(run.grid, run.font_index)?;

        // Make room for the attributed string, CTTypesetter, and CTLine.
        // try self.cf_release_pool.ensureUnusedCapacity(self.alloc, 4);
        self.cf_release_pool.reserve(4);

        // const str = macos.foundation.String.createWithCharactersNoCopy(state.unichars.items);
        // self.cf_release_pool.appendAssumeCapacity(str);
        //
        // PORT NOTE (no-copy): CoreText references `state.unichars` directly (no
        // copy, `kCFAllocatorNull` deallocator so CF never frees our buffer). The
        // string's character memory is only read while the typesetter/line are
        // built below (before `unichars` is next mutated), and the pooled `str` is
        // only ever `CFRelease`d afterwards (which never reads the buffer) — so the
        // no-copy view is sound, exactly as in the reference.
        let str = unsafe {
            CFString::with_characters_no_copy(
                None,
                self.run_state.unichars.as_ptr(),
                self.run_state.unichars.len() as CFIndex,
                kCFAllocatorNull,
            )
        }
        .ok_or(Error::OutOfMemory)?;
        self.cf_release_pool
            .push(CFRetained::into_raw(str).as_ptr() as *mut c_void);
        // Re-borrow the pooled string for building the attributed string.
        let str_ref: &CFString =
            unsafe { &*(*self.cf_release_pool.last().unwrap() as *const CFString) };

        // Create an attributed string from our string
        // const attr_str = try macos.foundation.AttributedString.create(str, attr_dict);
        // self.cf_release_pool.appendAssumeCapacity(attr_str);
        let attr_str = unsafe { CFAttributedString::new(None, Some(str_ref), Some(&*attr_dict)) }
            .ok_or(Error::OutOfMemory)?;
        self.cf_release_pool
            .push(CFRetained::into_raw(attr_str).as_ptr() as *mut c_void);
        let attr_str_ref: &CFAttributedString =
            unsafe { &*(*self.cf_release_pool.last().unwrap() as *const CFAttributedString) };

        // Create a typesetter from the attributed string and the cached
        // attr dict. (See comment in init for more info on the attr dict.)
        // const typesetter = try macos.text.Typesetter.createWithAttributedStringAndOptions(attr_str, self.typesetter_attr_dict);
        // self.cf_release_pool.appendAssumeCapacity(typesetter);
        let typesetter = unsafe {
            CTTypesetter::with_attributed_string_and_options(
                attr_str_ref,
                Some(&self.typesetter_attr_dict),
            )
        }
        .ok_or(Error::OutOfMemory)?;
        self.cf_release_pool
            .push(CFRetained::into_raw(typesetter).as_ptr() as *mut c_void);
        let typesetter_ref: &CTTypesetter =
            unsafe { &*(*self.cf_release_pool.last().unwrap() as *const CTTypesetter) };

        // Create a line from the typesetter
        // const line = typesetter.createLine(.{ .location = 0, .length = 0 });
        // self.cf_release_pool.appendAssumeCapacity(line);
        let line: CFRetained<CTLine> = unsafe {
            typesetter_ref.line(CFRange {
                location: 0,
                length: 0,
            })
        };
        self.cf_release_pool
            .push(CFRetained::into_raw(line).as_ptr() as *mut c_void);
        let line_ref: &CTLine =
            unsafe { &*(*self.cf_release_pool.last().unwrap() as *const CTLine) };

        // This keeps track of the current x offset (sum of advance.width) and
        // the furthest cluster we've seen so far (max).
        // var run_offset: Offset = .{};
        let mut run_offset: Offset = Offset::default();

        // This keeps track of the cell starting x and cluster.
        // var cell_offset: Offset = .{};
        let mut cell_offset: Offset = Offset::default();

        // For debugging positions, turn this on:
        //var run_offset_y: f64 = 0.0;
        //var cell_offset_y: f64 = 0.0;

        // Clear our cell buf and make sure we have enough room for the whole
        // line of glyphs, so that we can just assume capacity when appending
        // instead of maybe allocating.
        // self.cell_buf.clearRetainingCapacity();
        self.cell_buf.clear();
        // try self.cell_buf.ensureTotalCapacity(self.alloc, line.getGlyphCount());
        self.cell_buf
            .reserve(unsafe { line_ref.glyph_count() }.max(0) as usize);

        // CoreText, despite our insistence with an enforced embedding level,
        // may sometimes output runs that are non-monotonic. In order to fix
        // this, we check the run status for each run and if any aren't ltr
        // we set this to true, which indicates that we must sort our buffer.
        // var non_ltr: bool = false;
        let mut non_ltr: bool = false;

        // CoreText may generate multiple runs even though our input to
        // CoreText is already split into runs by our own run iterator.
        // The runs as far as I can tell are always sequential to each
        // other so we can iterate over them and just append to our
        // cell buffer.
        // const runs = line.getGlyphRuns();
        let runs = unsafe { line_ref.glyph_runs() };
        // for (0..runs.getCount()) |run_i| {
        let runs_count = runs.count();
        for run_i in 0..runs_count {
            // const ctrun = runs.getValueAtIndex(macos.text.Run, run_i);
            let ctrun: &CTRun = unsafe { &*(runs.value_at_index(run_i) as *const CTRun) };

            // const status = ctrun.getStatus();
            // if (status.non_monotonic or status.right_to_left) non_ltr = true;
            let status: CTRunStatus = unsafe { ctrun.status() };
            if (status.0 & CTRunStatus::NonMonotonic.0) != 0
                || (status.0 & CTRunStatus::RightToLeft.0) != 0
            {
                non_ltr = true;
            }

            // Get our glyphs and positions
            // const glyphs = ctrun.getGlyphsPtr() orelse try ctrun.getGlyphs(alloc);
            // const advances = ctrun.getAdvancesPtr() orelse try ctrun.getAdvances(alloc);
            // const positions = ctrun.getPositionsPtr() orelse try ctrun.getPositions(alloc);
            // const indices = ctrun.getStringIndicesPtr() orelse try ctrun.getStringIndices(alloc);
            let count = unsafe { ctrun.glyph_count() }.max(0) as usize;

            let glyphs_v: Vec<CGGlyph>;
            let glyphs: &[CGGlyph] =
                match NonNull::new(unsafe { ctrun.glyphs_ptr() } as *mut CGGlyph) {
                    Some(p) => unsafe { slice::from_raw_parts(p.as_ptr(), count) },
                    None => {
                        glyphs_v = run_glyphs(ctrun, count);
                        &glyphs_v
                    }
                };
            let advances_v: Vec<CGSize>;
            let advances: &[CGSize] =
                match NonNull::new(unsafe { ctrun.advances_ptr() } as *mut CGSize) {
                    Some(p) => unsafe { slice::from_raw_parts(p.as_ptr(), count) },
                    None => {
                        advances_v = run_advances(ctrun, count);
                        &advances_v
                    }
                };
            let positions_v: Vec<CGPoint>;
            let positions: &[CGPoint] =
                match NonNull::new(unsafe { ctrun.positions_ptr() } as *mut CGPoint) {
                    Some(p) => unsafe { slice::from_raw_parts(p.as_ptr(), count) },
                    None => {
                        positions_v = run_positions(ctrun, count);
                        &positions_v
                    }
                };
            let indices_v: Vec<CFIndex>;
            let indices: &[CFIndex] =
                match NonNull::new(unsafe { ctrun.string_indices_ptr() } as *mut CFIndex) {
                    Some(p) => unsafe { slice::from_raw_parts(p.as_ptr(), count) },
                    None => {
                        indices_v = run_string_indices(ctrun, count);
                        &indices_v
                    }
                };
            // assert(glyphs.len == advances.len);
            // assert(glyphs.len == positions.len);
            // assert(glyphs.len == indices.len);
            assert(glyphs.len() == advances.len());
            assert(glyphs.len() == positions.len());
            assert(glyphs.len() == indices.len());

            // for (glyphs, advances, positions, indices) |glyph, advance, position, index| {
            for i in 0..glyphs.len() {
                let glyph = glyphs[i];
                let advance = advances[i];
                let position = positions[i];
                let index = indices[i] as usize;

                // Our cluster is also our cell X position. If the cluster changes
                // then we need to reset our current cell offsets.
                // const cluster = state.codepoints.items[index].cluster;
                let cluster = self.run_state.codepoints[index].cluster;
                // if (cell_offset.cluster != cluster) {
                if cell_offset.cluster != cluster {
                    // We previously asserted that the new cluster is greater
                    // than cell_offset.cluster, but this isn't always true.
                    // See e.g. the "shape Chakma vowel sign with ligature
                    // (vowel sign renders first)" test.

                    // const is_after_glyph_from_current_or_next_clusters = cluster <= run_offset.cluster;
                    let is_after_glyph_from_current_or_next_clusters =
                        cluster <= run_offset.cluster;

                    // const is_first_codepoint_in_cluster = blk: {
                    //     var i = index;
                    //     while (i > 0) {
                    //         i -= 1;
                    //         const codepoint = state.codepoints.items[i];
                    //         // Skip surrogate pair padding
                    //         if (codepoint.codepoint == 0) continue;
                    //         break :blk codepoint.cluster != cluster;
                    //     } else break :blk true;
                    // };
                    let is_first_codepoint_in_cluster = 'blk: {
                        let mut i = index;
                        while i > 0 {
                            i -= 1;
                            let codepoint = self.run_state.codepoints[i];

                            // Skip surrogate pair padding
                            if codepoint.codepoint == 0 {
                                continue;
                            }
                            break 'blk codepoint.cluster != cluster;
                        }
                        true
                    };

                    // We need to reset the `cell_offset` at the start of a new
                    // cluster, but we do that conditionally if the codepoint
                    // `is_first_codepoint_in_cluster` and the cluster is not
                    // `is_after_glyph_from_current_or_next_clusters`, which is
                    // a heuristic to detect ligatures and avoid positioning
                    // glyphs that mark ligatures incorrectly. The idea is that
                    // if the first codepoint in a cluster doesn't appear in
                    // the stream, it's very likely that it combined with
                    // codepoints from a previous cluster into a ligature.
                    // Then, the subsequent codepoints are very likely marking
                    // glyphs that are placed relative to that ligature, so if
                    // we were to reset the `cell_offset` to align it with the
                    // grid, the positions would be off. The
                    // `!is_after_glyph_from_current_or_next_clusters` check is
                    // needed in case these marking glyphs come from a later
                    // cluster but are rendered first (see the Chakma and
                    // Bengali tests). In that case when we get to the
                    // codepoint that `is_first_codepoint_in_cluster`, but in a
                    // cluster that
                    // `is_after_glyph_from_current_or_next_clusters`, we don't
                    // want to reset to the grid and cause the positions to be
                    // off. (Note that we could go back and align the cells to
                    // the grid starting from the one from the cluster that
                    // rendered out of order, but that is more complicated so
                    // we don't do that for now. Also, it's TBD if there are
                    // exceptions to this heuristic for detecting ligatures,
                    // but using the logging below seems to show it works
                    // well.)
                    // if (is_first_codepoint_in_cluster and !is_after_glyph_from_current_or_next_clusters) {
                    if is_first_codepoint_in_cluster
                        && !is_after_glyph_from_current_or_next_clusters
                    {
                        // cell_offset = .{ .cluster = cluster, .x = run_offset.x };
                        cell_offset = Offset {
                            cluster,
                            x: run_offset.x,
                        };

                        // For debugging positions, turn this on:
                        //cell_offset_y = run_offset_y;
                    }
                }

                // For debugging positions, turn this on:
                //try self.debugPositions(alloc, run_offset, run_offset_y, cell_offset, cell_offset_y, position, index);

                // const x_offset = position.x - cell_offset.x;
                let x_offset = position.x - cell_offset.x;

                // self.cell_buf.appendAssumeCapacity(.{
                //     .x = @intCast(cell_offset.cluster),
                //     .x_offset = @intFromFloat(@round(x_offset)),
                //     .y_offset = @intFromFloat(@round(position.y)),
                //     .glyph_index = glyph,
                // });
                self.cell_buf.push(shape::Cell {
                    x: cell_offset.cluster as u16,
                    x_offset: x_offset.round() as i16,
                    y_offset: position.y.round() as i16,
                    glyph_index: glyph as u32,
                });

                // Add our advances to keep track of our run offsets.
                // Advances apply to the NEXT cell.
                // run_offset.x += advance.width;
                run_offset.x += advance.width;
                // run_offset.cluster = @max(run_offset.cluster, cluster);
                run_offset.cluster = run_offset.cluster.max(cluster);

                // For debugging positions, turn this on:
                //run_offset_y += advance.height;
            }
        }

        // If our buffer contains some non-ltr sections we need to sort it :/
        // if (non_ltr) {
        if non_ltr {
            // This is EXCEPTIONALLY rare. Only happens for languages with
            // complex shaping which we don't even really support properly
            // right now, so are very unlikely to be used heavily by users
            // of Ghostty.
            // @branchHint(.cold);
            // std.mem.sort(font.shape.Cell, self.cell_buf.items, {}, struct {
            //     fn lt(_: void, a: font.shape.Cell, b: font.shape.Cell) bool {
            //         return a.x < b.x;
            //     }
            // }.lt);
            self.cell_buf.sort_by(|a, b| a.x.cmp(&b.x));
        }

        // return self.cell_buf.items;
        Ok(&self.cell_buf)
    }

    /// Get an attr dict for a font from a specific index.
    /// These items are cached, do not retain or release them.
    // fn getFont(self: *Shaper, grid: *font.SharedGrid, index: font.Collection.Index) !*macos.foundation.Dictionary {
    fn get_font(
        &mut self,
        grid: *mut SharedGrid,
        index: collection::Index,
    ) -> Result<*const CFDictionary, Error> {
        // If this grid doesn't match the one we've cached fonts for,
        // then we reset the cache list since it's no longer valid.
        // We use an intFromPtr rather than direct pointer comparison
        // because we don't want anyone to inadvertently use the pointer.
        // const grid_id: usize = @intFromPtr(grid);
        let grid_id: usize = grid as usize;
        // if (grid_id != self.cached_font_grid) {
        if grid_id != self.cached_font_grid {
            // if (self.cached_font_grid > 0) {
            if self.cached_font_grid > 0 {
                // Put all the currently cached fonts in to
                // the release pool before clearing the list.
                // try self.cf_release_pool.ensureUnusedCapacity(self.alloc, self.cached_fonts.items.len);
                self.cf_release_pool.reserve(self.cached_fonts.len());
                // for (self.cached_fonts.items) |ft| if (ft) |f| self.cf_release_pool.appendAssumeCapacity(f);
                for f in self.cached_fonts.drain(..).flatten() {
                    self.cf_release_pool
                        .push(CFRetained::into_raw(f).as_ptr() as *mut c_void);
                }
                // self.cached_fonts.clearRetainingCapacity();
                // (the `drain(..)` above already emptied the list.)
            }

            // self.cached_font_grid = grid_id;
            self.cached_font_grid = grid_id;
        }

        // const index_int = index.int();
        let index_int = index.int() as usize;

        // The cached fonts are indexed directly by the font index, since
        // this number is usually low. Therefore, we set any index we haven't
        // seen to null.
        // if (self.cached_fonts.items.len <= index_int) {
        //     try self.cached_fonts.ensureTotalCapacity(self.alloc, index_int + 1);
        //     while (self.cached_fonts.items.len <= index_int) self.cached_fonts.appendAssumeCapacity(null);
        // }
        if self.cached_fonts.len() <= index_int {
            self.cached_fonts
                .reserve(index_int + 1 - self.cached_fonts.len());
            while self.cached_fonts.len() <= index_int {
                self.cached_fonts.push(None);
            }
        }

        // If we have it, return the cached attr dict.
        // if (self.cached_fonts.items[index_int]) |cached| return cached;
        if let Some(cached) = &self.cached_fonts[index_int] {
            return Ok(CFRetained::as_ptr(cached).as_ptr() as *const CFDictionary);
        }

        // Font descriptor, font
        // try self.cf_release_pool.ensureUnusedCapacity(self.alloc, 2);
        self.cf_release_pool.reserve(2);

        // const run_font = font: {
        let run_font = {
            // The CoreText shaper relies on CoreText and CoreText claims
            // that CTFonts are threadsafe. See:
            // https://developer.apple.com/documentation/coretext/
            //
            // Quote:
            // All individual functions in Core Text are thread-safe. Font
            // objects (CTFont, CTFontDescriptor, and associated objects) can
            // be used simultaneously by multiple operations, work queues, or
            // threads. However, the layout objects (CTTypesetter,
            // CTFramesetter, CTRun, CTLine, CTFrame, and associated objects)
            // should be used in a single operation, work queue, or thread.
            //
            // Because of this, we only acquire the read lock to grab the
            // face and set it up, then release it.
            // grid.lock.lockShared();
            // defer grid.lock.unlockShared();
            let _guard = unsafe { (*grid).lock.read().unwrap() };

            // const face = try grid.resolver.collection.getFace(index);
            let face = unsafe { (*grid).resolver.collection.get_face(index)? };
            // const original = face.font;
            let original = unsafe { &(*face).font };

            // const attrs = if (face.quirks_disable_default_font_features)
            //     self.features_no_default else self.features;
            let attrs: &CFDictionary = if unsafe { (*face).quirks_disable_default_font_features } {
                &self.features_no_default
            } else {
                &self.features
            };

            // const desc = try macos.text.FontDescriptor.createWithAttributes(attrs);
            // self.cf_release_pool.appendAssumeCapacity(desc);
            let desc = unsafe { CTFontDescriptor::with_attributes(attrs) };
            let desc_raw = CFRetained::into_raw(desc);
            self.cf_release_pool.push(desc_raw.as_ptr() as *mut c_void);
            let desc_ref: &CTFontDescriptor = unsafe { desc_raw.as_ref() };

            // const copied = try original.copyWithAttributes(0, null, desc);
            // errdefer copied.release();
            // break :font copied;
            unsafe { original.copy_with_attributes(0.0, ptr::null(), Some(desc_ref)) }
        };
        // self.cf_release_pool.appendAssumeCapacity(run_font);
        let run_font_raw = CFRetained::into_raw(run_font);
        self.cf_release_pool
            .push(run_font_raw.as_ptr() as *mut c_void);
        let run_font_ref = unsafe { run_font_raw.as_ref() };

        // Get our font and use that get the attributes to set for the
        // attributed string so the whole string uses the same font.
        // const attr_dict = dict: {
        //     break :dict try macos.foundation.Dictionary.create(
        //         &.{macos.text.StringAttribute.font.key()},
        //         &.{run_font},
        //     );
        // };
        let mut keys: [*const c_void; 1] =
            [unsafe { kCTFontAttributeName } as *const CFString as *const c_void];
        let mut values: [*const c_void; 1] =
            [run_font_ref as *const objc2_core_text::CTFont as *const c_void];
        let attr_dict =
            unsafe { make_cf_dict(&mut keys, &mut values) }.ok_or(Error::OutOfMemory)?;

        // self.cached_fonts.items[index_int] = attr_dict;
        // return attr_dict;
        let ret = CFRetained::as_ptr(&attr_dict).as_ptr() as *const CFDictionary;
        self.cached_fonts[index_int] = Some(attr_dict);
        Ok(ret)
    }

    // fn debugPositions(...) !void { ... }
    //
    // PORT NOTE (log-as-comment): `debugPositions` is entirely diagnostic — it
    // computes formatted codepoint strings and emits `log.warn`s (gated on
    // `positions_differ`/`position_y_differs`/`cluster_differs`/`extra_debugging`),
    // and its sole call site (in `shape`, above) is already commented out. This
    // crate wires no logger, and the function reads
    // `unicode.table.get(...).grapheme_boundary_class` which is not part of this
    // tier. It is therefore preserved verbatim as a comment (R1: no loss):
    //
    // fn debugPositions(
    //     self: *Shaper,
    //     alloc: Allocator,
    //     run_offset: Offset,
    //     run_offset_y: f64,
    //     cell_offset: Offset,
    //     cell_offset_y: f64,
    //     position: macos.graphics.Point,
    //     index: usize,
    // ) !void {
    //     const state = &self.run_state;
    //     const x_offset = position.x - cell_offset.x;
    //     const advance_x_offset = run_offset.x - cell_offset.x;
    //     const advance_y_offset = run_offset_y - cell_offset_y;
    //     const x_offset_diff = x_offset - advance_x_offset;
    //     const y_offset_diff = position.y - advance_y_offset;
    //     const positions_differ = @abs(x_offset_diff) > 0.0001 or @abs(y_offset_diff) > 0.0001;
    //     const old_offset_y = position.y - cell_offset_y;
    //     const position_y_differs = @abs(cell_offset_y) > 0.0001;
    //     const cluster = state.codepoints.items[index].cluster;
    //     const cluster_differs = cluster != cell_offset.cluster;
    //
    //     // To debug every loop, flip this to true:
    //     const extra_debugging = false;
    //
    //     const is_previous_codepoint_prepend = if (cluster_differs or extra_debugging) blk: {
    //         var i = index;
    //         while (i > 0) {
    //             i -= 1;
    //             const codepoint = state.codepoints.items[i];
    //             // Skip surrogate pair padding
    //             if (codepoint.codepoint == 0) continue;
    //             break :blk unicode.table.get(@intCast(codepoint.codepoint)).grapheme_boundary_class == .prepend;
    //         }
    //         break :blk false;
    //     } else false;
    //
    //     const formatted_cps = if (positions_differ or position_y_differs or cluster_differs or extra_debugging) blk: {
    //         var allocating = std.Io.Writer.Allocating.init(alloc);
    //         const writer = &allocating.writer;
    //         const codepoints = state.codepoints.items;
    //         var last_cluster: ?u32 = null;
    //         for (codepoints, 0..) |cp, i| {
    //             if ((@as(i32, @intCast(cp.cluster)) >= @as(i32, @intCast(cell_offset.cluster)) - 1 and
    //                 cp.cluster <= cluster + 1) and cp.codepoint != 0) {
    //                 if (last_cluster) |last| if (cp.cluster != last) try writer.writeAll(" ");
    //                 if (i == index) try writer.writeAll("▸");
    //                 // Using Python syntax for easier debugging
    //                 if (cp.codepoint > 0xFFFF) try writer.print("\\U{x:0>8}", .{cp.codepoint})
    //                 else try writer.print("\\u{x:0>4}", .{cp.codepoint});
    //                 last_cluster = cp.cluster;
    //             }
    //         }
    //         try writer.writeAll(" → ");
    //         for (codepoints) |cp| {
    //             if ((@as(i32, @intCast(cp.cluster)) >= @as(i32, @intCast(cell_offset.cluster)) - 1 and
    //                 cp.cluster <= cluster + 1) and cp.codepoint != 0) {
    //                 try writer.print("{u}", .{@as(u21, @intCast(cp.codepoint))});
    //             }
    //         }
    //         break :blk try allocating.toOwnedSlice();
    //     } else "";
    //
    //     if (extra_debugging) {
    //         log.warn("extra debugging of positions index={d} cell_offset.cluster={d} cluster={d} run_offset.cluster={d} diff={d} pos=({d:.2},{d:.2}) run_offset=({d:.2},{d:.2}) cell_offset=({d:.2},{d:.2}) is_prev_prepend={} cps = {s}", .{
    //             index, cell_offset.cluster, cluster, run_offset.cluster,
    //             @as(isize, @intCast(cluster)) - @as(isize, @intCast(cell_offset.cluster)),
    //             x_offset, position.y, run_offset.x, run_offset_y, cell_offset.x, cell_offset_y,
    //             is_previous_codepoint_prepend, formatted_cps,
    //         });
    //     }
    //
    //     if (positions_differ) {
    //         log.warn("position differs from advance: cluster={d} pos=({d:.2},{d:.2}) adv=({d:.2},{d:.2}) diff=({d:.2},{d:.2}) cps = {s}", .{
    //             cluster, x_offset, position.y, advance_x_offset, advance_y_offset, x_offset_diff, y_offset_diff, formatted_cps,
    //         });
    //     }
    //
    //     if (position_y_differs) {
    //         log.warn("position.y differs from old offset.y: cluster={d} pos=({d:.2},{d:.2}) run_offset=({d:.2},{d:.2}) cell_offset=({d:.2},{d:.2}) old offset.y={d:.2} cps = {s}", .{
    //             cluster, x_offset, position.y, run_offset.x, run_offset_y, cell_offset.x, cell_offset_y, old_offset_y, formatted_cps,
    //         });
    //     }
    //
    //     if (cluster_differs) {
    //         log.warn("cell_offset.cluster differs from cluster (potential ligature detected) cell_offset.cluster={d} cluster={d} run_offset.cluster={d} diff={d} pos=({d:.2},{d:.2}) run_offset=({d:.2},{d:.2}) cell_offset=({d:.2},{d:.2}) is_prev_prepend={} cps = {s}", .{
    //             cell_offset.cluster, cluster, run_offset.cluster,
    //             @as(isize, @intCast(cluster)) - @as(isize, @intCast(cell_offset.cluster)),
    //             x_offset, position.y, run_offset.x, run_offset_y, cell_offset.x, cell_offset_y,
    //             is_previous_codepoint_prepend, formatted_cps,
    //         });
    //     }
    // }
}

/// The hooks for RunIterator.
// pub const RunIteratorHook = struct { shaper: *Shaper, ... };
pub struct RunIteratorHook {
    pub shaper: *mut Shaper,
}

impl shape::run::RunIteratorHook for RunIteratorHook {
    // pub fn prepare(self: *RunIteratorHook) void {
    fn prepare(&mut self) {
        // self.shaper.run_state.reset();
        unsafe { (*self.shaper).run_state.reset() };
        // log.warn("----------- run reset -------------", .{});
    }

    // pub fn addCodepoint(self: RunIteratorHook, cp: u32, cluster: u32) !void {
    fn add_codepoint(&mut self, cp: u32, cluster: u32) {
        // const state = &self.shaper.run_state;
        let state = unsafe { &mut (*self.shaper).run_state };

        // Build our UTF-16 string for CoreText
        // try state.unichars.ensureUnusedCapacity(self.shaper.alloc, 2);
        state.unichars.reserve(2);

        // state.unichars.appendNTimesAssumeCapacity(0, 2);
        let base = state.unichars.len();
        state.unichars.push(0);
        state.unichars.push(0);

        // const pair = macos.foundation.stringGetSurrogatePairForLongCharacter(
        //     cp, state.unichars.items[state.unichars.items.len - 2 ..][0..2]);
        let pair =
            string_get_surrogate_pair_for_long_character(cp, &mut state.unichars[base..base + 2]);
        // if (!pair) state.unichars.items.len -= 1;
        if !pair {
            state.unichars.pop();
        }

        // Build our reverse lookup table for codepoints to clusters
        // try state.codepoints.append(self.shaper.alloc, .{ .codepoint = cp, .cluster = cluster });
        state.codepoints.push(Codepoint {
            codepoint: cp,
            cluster,
        });
        // log.warn("run cp={X}", .{cp});

        // If the UTF-16 codepoint is a pair then we need to insert
        // a dummy entry so that the CTRunGetStringIndices() function
        // maps correctly.
        // if (pair) try state.codepoints.append(self.shaper.alloc, .{ .codepoint = 0, .cluster = cluster });
        if pair {
            state.codepoints.push(Codepoint {
                codepoint: 0,
                cluster,
            });
        }
    }

    // pub fn finalize(self: RunIteratorHook) void { _ = self; }
    fn finalize(&mut self) {}
}

/// UTF-32 -> UTF-16 surrogate decomposition, matching CoreFoundation.
// PORT NOTE: `macos.foundation.stringGetSurrogatePairForLongCharacter` calls the
// `CF_INLINE` `CFStringGetSurrogatePairForLongCharacter` (not an exported dynamic
// symbol; objc2 leaves it a TODO). Its header body is reproduced verbatim here —
// the same helper `face/coretext.rs` uses — writing into the two `surrogates`
// slots and returning whether a non-BMP scalar was split.
fn string_get_surrogate_pair_for_long_character(character: u32, surrogates: &mut [u16]) -> bool {
    if (character > 0xFFFF) && (character < 0x110000) {
        // Non-BMP character
        let character = character - 0x10000;
        surrogates[0] = ((character >> 10) + 0xD800) as u16;
        surrogates[1] = ((character & 0x3FF) + 0xDC00) as u16;
        true
    } else {
        surrogates[0] = character as u16;
        false
    }
}

/// Build a CoreFoundation dictionary with the standard CF-type key/value
/// callbacks (retain/release), matching `macos.foundation.Dictionary.create`.
// PORT NOTE: `Dictionary.create(keys, values)` passes `kCFTypeDictionaryKeyCallBacks`
// / `kCFTypeDictionaryValueCallBacks` (the retaining callbacks). Reproduced here.
unsafe fn make_cf_dict(
    keys: &mut [*const c_void],
    values: &mut [*const c_void],
) -> Option<CFRetained<CFDictionary>> {
    unsafe {
        CFDictionary::new(
            None,
            keys.as_mut_ptr(),
            values.as_mut_ptr(),
            keys.len() as CFIndex,
            &raw const kCFTypeDictionaryKeyCallBacks,
            &raw const kCFTypeDictionaryValueCallBacks,
        )
    }
}

// const glyphs = ctrun.getGlyphsPtr() orelse try ctrun.getGlyphs(alloc);
// PORT NOTE: the four `getXPtr() orelse getX(alloc)` fallbacks — when CoreText
// won't hand back a direct pointer, `getX(alloc)` allocates a length-`count`
// buffer and fills it via `CTRunGetX(run, {0,0}, buf)`. Reproduced as these
// helpers (each fills a `Vec<T>` via the `CFRange{0,0}` = whole-run getter).
fn run_glyphs(ctrun: &CTRun, count: usize) -> Vec<CGGlyph> {
    let mut buf: Vec<CGGlyph> = vec![0; count];
    if let Some(p) = NonNull::new(buf.as_mut_ptr()) {
        unsafe {
            ctrun.glyphs(
                CFRange {
                    location: 0,
                    length: 0,
                },
                p,
            )
        };
    }
    buf
}

fn run_advances(ctrun: &CTRun, count: usize) -> Vec<CGSize> {
    let mut buf: Vec<CGSize> = vec![
        CGSize {
            width: 0.0,
            height: 0.0
        };
        count
    ];
    if let Some(p) = NonNull::new(buf.as_mut_ptr()) {
        unsafe {
            ctrun.advances(
                CFRange {
                    location: 0,
                    length: 0,
                },
                p,
            )
        };
    }
    buf
}

fn run_positions(ctrun: &CTRun, count: usize) -> Vec<CGPoint> {
    let mut buf: Vec<CGPoint> = vec![CGPoint { x: 0.0, y: 0.0 }; count];
    if let Some(p) = NonNull::new(buf.as_mut_ptr()) {
        unsafe {
            ctrun.positions(
                CFRange {
                    location: 0,
                    length: 0,
                },
                p,
            )
        };
    }
    buf
}

fn run_string_indices(ctrun: &CTRun, count: usize) -> Vec<CFIndex> {
    let mut buf: Vec<CFIndex> = vec![0; count];
    if let Some(p) = NonNull::new(buf.as_mut_ptr()) {
        unsafe {
            ctrun.string_indices(
                CFRange {
                    location: 0,
                    length: 0,
                },
                p,
            )
        };
    }
    buf
}

/// The behavior-equivalent CFRelease thread (deferred-seam stand-in for
/// `os.CFReleaseThread`; see the module-level import note). A dedicated
/// `std::thread` drains an `mpsc` channel of release pools and `CFRelease`s
/// their refs, stopping on the `Stop` message. Identical observable behavior to
/// the reference's libxev-backed thread — no parity surface.
pub(crate) mod cf_release {
    use core::ffi::c_void;

    /// A pool of CFTypeRefs to release. CFTypeRefs are safe to `CFRelease` from
    /// any thread (CoreFoundation objects are thread-safe to release), so this
    /// wrapper is `Send`.
    pub struct SendableRefs(pub Vec<*mut c_void>);
    // SAFETY: CFRelease is thread-safe for CFTypeRefs; the refs are only ever
    // released (never dereferenced) on the receiving thread.
    unsafe impl Send for SendableRefs {}

    // PORT NOTE: the reference `CFReleaseThread.Message` union (`.release`), plus the
    // `stop` async modeled as the `Stop` message.
    pub enum Msg {
        Release(SendableRefs),
        Stop,
    }

    // PORT NOTE: `macos.foundation.CFRelease`. R3 binding to the SAME framework.
    extern "C" {
        pub fn CFRelease(cf: *mut c_void);
    }

    // PORT NOTE: `CFReleaseThread.threadMain` -> `drainMailbox` (the `.release` arm
    // `for (msg.refs) |ref| CFRelease(ref)`), looping until `stop`.
    pub fn thread_main(rx: std::sync::mpsc::Receiver<Msg>) {
        while let Ok(msg) = rx.recv() {
            match msg {
                Msg::Release(refs) => {
                    for r in refs.0 {
                        unsafe { CFRelease(r) };
                    }
                }
                Msg::Stop => break,
            }
        }
    }
}

#[cfg(test)]
#[path = "coretext_tests.rs"]
mod tests;
