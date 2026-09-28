//! rustty-font-shaper
//!
//! PORTS: ghostty `src/font/` shaping + backend-face layer that sits ABOVE
//! `rustty-vt` + `rustty-font` — the D1 three-tier split (memory
//! `apc-glyf-font-layering-d1`). `rustty-font` stays a PURE parsing/sprite leaf
//! BELOW `rustty-vt`; this crate is the shaper tier that hosts the backend
//! `Face` selector (`face.zig`), the CoreText/FreeType faces, and HarfBuzz
//! shaping, so it may depend on both `rustty-vt` and `rustty-font` plus the
//! `-sys`/objc2 backends.
//!
//! PORT STATUS: ported (the FULL D1 shaper tier — the test count depends on the
//! backend built, see the G-FONTBACKEND lane; "110 tests" here was stale by
//! S-14; the old
//! "F3 — shape.zig lands later" sentence was stale doc drift, realigned per
//! VALIDATION_REPORT.md §3b): `face.zig` (the backend selector) + the
//! `face/freetype.zig` and `face/coretext.zig` (macOS) backend faces,
//! `shape.zig` + the `shaper/` HarfBuzz framework (`run`/`feature`/`Cache`/
//! `noop`), `discovery.zig` (CoreText discoverer + `Descriptor`),
//! `Collection.zig`/`CodepointResolver.zig`, and `SharedGrid.zig`/
//! `SharedGridSet.zig`, plus the Fontconfig `DeferredFace` arms and discoverer
//! (Linux), the Windows arms, and the wasm `face/web_canvas.zig` +
//! `shaper/web_canvas.zig` backends. (CORRECTION, 2026-09-24, parity tail
//! S-8a: this header still listed the Fontconfig arms/discovery and
//! `face/web_canvas.zig` as an unported residual; all three are ported —
//! `deferred_face.rs` `fontconfig_payload`, `discovery.rs` `fontconfig_impl`,
//! `face/web_canvas.rs`.)
//!
//! The backend is a real build option (S-14, closing NEW-FINDINGS N266):
//! `-Dfont-backend` is `rustty-build-config`'s `font-backend-*` feature set,
//! `options::BACKEND` and every `switch (options.backend)` selector are keyed on
//! the `rustty_font_backend` cfg `build.rs` derives from it, and the non-default
//! arms are live — `loadCoreTextFreetype` (`coretext_freetype`), the CoreText
//! face's `harfbuzz_shaper` arms (`coretext_harfbuzz`), the noop shaper
//! (`coretext_noshape`) and the no-discovery arms (`freetype`). Built and
//! tested on macOS for all seven native backends — including
//! `fontconfig_freetype` and `freetype_windows`, with host fixtures (the
//! G-FONTBACKEND lane); type-checked, not run, for Linux and Windows
//! `freetype`. Not built: the CoreText backends off Darwin (no CoreText),
//! `fontconfig_freetype` on an MSVC-ABI Windows target (the reference's
//! fontconfig does not compile there either; on a GNU-ABI Windows target it is
//! built and tested, N286), and a native `web_canvas`
//! (no feature). The FreeType arm of the reference's never-run `face metrics`
//! test holds stale values (N285 / UD281; faithful). (CORRECTION, S-14b: this
//! listed `fontconfig_freetype` on macOS and `freetype_windows` off Windows as
//! not built.)
//!
//! Carved leaves-first (R1: every line/comment/test{} ported as the crate is
//! carved, under the acyclicity gate per R4).

// RUSTTY-ONLY, TEST-ONLY (task-14c-4): the capture logger that proves this
// crate's restored `log::` call sites actually emit. Never compiled into the
// production library. See the module header.
#[cfg(test)]
mod log_capture;

// RUSTTY-ONLY, TEST-ONLY (task T-F2): re-run one test in a fresh child process
// (CoreText's process-wide font state). See the module header.
#[cfg(all(test, target_os = "macos"))]
mod test_child;

/// RUSTTY-ONLY (no oracle analogue): install `rustty-os`'s wasm `log::Log`
/// backend for the wasm artifacts this crate is linked into.
///
/// `.port/parity-closure/DECISION-logging.md` §6r3 finding 3 established the
/// rule — a restored `log::` site emits nothing unless the RUNNING ARTIFACT has
/// registered a backend, and the registration belongs at the artifact's first
/// JS-callable entry point. `rustty_web_canvas.wasm` had none: it is rooted at
/// `rustty-web-canvas`, and a JS host that drives the FONT surface never
/// touches `rustty-vt-c`'s `rustty_terminal_*` exports, which is where the
/// other backend install calls live.
///
/// # This is an ARTIFACT-wide contract, not a crate-wide one
///
/// The registration must fire from EVERY unmangled entry point in the artifact,
/// not merely from this crate's own `rustty_wc_face_*` / `rustty_wc_shaper_*`
/// exports. `rustty-web-canvas` owns twelve more (`rustty_wc_atlas_*`,
/// `rustty_wc_deferred_*`, `rustty_wc_alloc`/`_free`), and at least one of them
/// reaches a restored site: `rustty_wc_deferred_has_codepoint` calls
/// `DeferredFace::has_codepoint`, whose web-canvas arm emits
/// `failed to init face for codepoint check`. A host driving only the deferred
/// surface would lose it. That is why this fn is `pub` and lives at the crate
/// ROOT (available on every target) rather than `pub(crate)` inside the
/// wasm32-only `face::web_canvas` — the visibility WAS the thing preventing
/// `rustty-web-canvas` from complying.
///
/// **If you add a new unmangled export to `rustty_web_canvas.wasm`, from either
/// crate, call this first.**
///
/// `rustty_os::wasm::log::install_log_backend` is `Once`-guarded and leaves an
/// already-installed logger's filter alone, so the repetition is free and
/// cannot clobber an embedder's own backend (§6r CRITICAL 1).
///
/// Non-wasm targets get a no-op: `rustty-os`'s wasm log module does not exist
/// there, and the native builds already have a backend installed by their own
/// binary. It is callable on every target so the call sites need no `cfg`.
#[inline]
pub fn install_wasm_log_backend() {
    #[cfg(target_arch = "wasm32")]
    rustty_os::wasm::log::install_log_backend();
}

/// RUSTTY-ONLY guard, the sibling of
/// `rustty-web-canvas`'s `install_contract_tests` (task-14c-4 fix round 1, F3).
/// The `rustty_web_canvas.wasm` log-backend contract is ARTIFACT-wide but its
/// entry points are split across two crates; each crate guards its own half,
/// because neither can `include_str!` the other's source. Keep the two in step.
#[cfg(test)]
mod install_contract_tests {
    const CALL: &str = "install_wasm_log_backend();";

    /// Every `rustty_wc_*` export this crate owns must call
    /// [`crate::install_wasm_log_backend`] as its FIRST statement.
    #[test]
    fn every_rustty_wc_export_installs_the_log_backend() {
        // (file label, source, expected export count)
        let files: [(&str, &str, usize); 2] = [
            (
                "face/web_canvas.rs",
                include_str!("face/web_canvas.rs"),
                4, // face_new, face_free, face_render_glyph, face_debug_canvas
            ),
            (
                "shaper/web_canvas.rs",
                include_str!("shaper/web_canvas.rs"),
                3, // shaper_new, shaper_free, shaper_shape
            ),
        ];

        for (label, src, expected) in files {
            let lines: Vec<&str> = src.lines().collect();
            let mut checked: Vec<&str> = Vec::new();
            let mut missing: Vec<&str> = Vec::new();

            for (i, line) in lines.iter().enumerate() {
                let t = line.trim_start();
                // Only the PREFIXED `rustty_wc_*` surface: the unprefixed
                // `ghostty-wasm-abi` twins belong to `rustty_wasm.wasm`, whose
                // backend `rustty-config`/`rustty-wasm` already install
                // (DECISION-logging.md §6r3 finding 3).
                if !(t.starts_with("pub extern \"C\" fn rustty_wc_")
                    || t.starts_with("pub unsafe extern \"C\" fn rustty_wc_"))
                {
                    continue;
                }
                let name = t
                    .rsplit("fn ")
                    .next()
                    .unwrap_or(t)
                    .split(['(', '<'])
                    .next()
                    .unwrap_or(t);

                let mut j = i;
                while j < lines.len() && !lines[j].trim_end().ends_with('{') {
                    j += 1;
                }
                let first_stmt = lines
                    .get(j + 1..)
                    .unwrap_or(&[])
                    .iter()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| l.trim())
                    .unwrap_or("");

                checked.push(name);
                if !first_stmt.ends_with(CALL) {
                    missing.push(name);
                }
            }

            // The scan must not silently find nothing (the vacuous-pass trap).
            assert_eq!(
                expected,
                checked.len(),
                "{label}: expected {expected} rustty_wc_* exports, scanned {checked:?}"
            );
            assert_eq!(
                Vec::<&str>::new(),
                missing,
                "{label}: these exports do not open with a call ending `{CALL}`"
            );
        }
    }
}

// RUSTTY-ONLY guard (no oracle analogue; the reference selects its font backend
// by build option and a mismatched one simply fails to compile). The
// `freetype`/`harfbuzz`/`fontconfig` features exist so an EMBEDDER can drop the
// vendored-C font stack from its resolve graph when the selected backend needs
// none of it (`.coretext`, the macOS default, or `.coretext_noshape`). A backend
// that DOES need it (`hasFreetype()`/`hasHarfbuzz()`/`hasFontconfig()`: every
// non-macOS default, and `-Dfont-backend=coretext_freetype`/`coretext_harfbuzz`
// on macOS) would otherwise produce a cascade of missing-module errors instead
// of one honest message. `build.rs` refuses the same combinations first; these
// keep the rule visible at the crate root. (S-14: keyed on the backend cfgs; they
// were keyed on the target OS while the option was folded to its default.)
#[cfg(all(rustty_font_has_freetype, not(feature = "freetype")))]
compile_error!(
    "rustty-font-shaper: the selected font backend has FreeType (`hasFreetype()`), \
     which needs the `freetype` feature."
);
#[cfg(all(rustty_font_has_harfbuzz, not(feature = "harfbuzz")))]
compile_error!(
    "rustty-font-shaper: the selected font backend shapes with HarfBuzz \
     (`hasHarfbuzz()`), which needs the `harfbuzz` feature."
);
#[cfg(all(rustty_font_has_fontconfig, not(feature = "fontconfig")))]
compile_error!(
    "rustty-font-shaper: the selected font backend is `.fontconfig_freetype`, \
     which needs the `fontconfig` feature."
);

pub mod backend;
pub mod codepoint_resolver;
pub mod collection;
// Local DI stand-ins for the `rustty-config` value types `SharedGridSet` reads
// (break #9: config is a HIGHER crate injected via DI; see the module doc).
pub mod config;
pub mod deferred_face;
pub mod discovery;
pub mod face;
// PORT NOTE: `shape.zig` (the `Shaper` interface + backend selector) and its
// `shaper/` sub-files (`run`/`feature`/`Cache`/`noop`) — the shaper framework.
pub mod shape;
// PORT NOTE: `SharedGrid.zig` (the shared glyph-atlas grid the renderer reads —
// `renderGlyph` caching over Collection+Atlas) and `SharedGridSet.zig` (the set
// keyed by config/font descriptors).
pub mod shared_grid;
pub mod shared_grid_set;
// The `pkg/freetype` and `pkg/harfbuzz` package ports (`unsafe` wrappers over the
// `-sys` crates; see each module's PORT NOTE). Both are now fully carved — the
// whole pinned public surface plus all of each package's own `test { ... }`
// blocks. `stb` remains the `src/stb` glue subset.
//
// WASM DIVERGENCE: the `wasm32` web_canvas backend has no FreeType/HarfBuzz/stb
// (freestanding wasm has no C deps), so these `-sys` FFI-glue modules are gated
// OFF; the `Face`/`Shaper`/`Library` selectors resolve to the browser Canvas
// backend instead (see `face`/`shape`/`library`). Native is untouched.
//
// EMBEDDER DIVERGENCE (features `freetype` / `harfbuzz`, both DEFAULT-ON): the
// same two modules drop out when an embedding application turns the vendored-C
// font backends off, because Cargo's `links` uniqueness check spans the whole
// resolve graph and an optional dependency is the only way to leave it. macOS
// resolves `Face`/`Shaper`/`Library` to the CoreText backend either way (the
// reference's own Darwin default is `.coretext`), so nothing is lost there; see
// the `[features]` header of this crate's `Cargo.toml`.
#[cfg(all(not(target_arch = "wasm32"), feature = "freetype"))]
pub mod ft;
#[cfg(all(not(target_arch = "wasm32"), feature = "harfbuzz"))]
pub mod hb;
// Fontconfig binding wrapper (port of `pkg/fontconfig/*.zig`), the
// font-discovery seam of the `fontconfig_freetype` backend. Compiled exactly on
// `hasFontconfig()` (`rustty_font_has_fontconfig`), as the reference imports
// `pkg/fontconfig` only for that backend (`SharedDeps.zig:292`): the Linux/BSD
// default, and `-Dfont-backend=fontconfig_freetype` on macOS. CORRECTION (S-14b,
// review M3): this was gated off macOS/Windows by `target_os`. An MSVC-ABI
// Windows target stays excluded by `rustty-build-config` (N286: the reference's
// fontconfig does not compile against the MSVC headers either); a GNU-ABI
// Windows target builds it, as the reference does. wasm has no C deps.
// EMBEDDER DIVERGENCE (feature `fontconfig`, DEFAULT-ON): an embedder that
// turns the vendored-C font backends off drops `fontconfig-sys` from the resolve
// graph, which is what a colliding `links = "fontconfig"` needs; `build.rs`
// refuses `fontconfig_freetype` without the feature.
#[cfg(all(rustty_font_has_fontconfig, feature = "fontconfig"))]
pub mod fc;
pub mod library;
#[cfg(not(target_arch = "wasm32"))]
pub mod stb;
// `uucode` seam (the single `is_emoji_presentation` property `CodepointResolver`
// reads). Glue, superseded by a full unicode-database port later.
pub mod uucode;

// The active backend `Face` type, selected by `face.zig`'s `switch` over the
// build backend (macOS default=coretext, Linux default=fontconfig_freetype).
pub use face::Face;

// PORT NOTE (shared face types): the reference `font` module (`main.zig`)
// re-exports `Metrics` and the glyph `RenderOptions` alongside the face
// `Options`, so the backend `Face` impls and shapers can name them unqualified
// (`Metrics = @import("Metrics.zig")`, `Glyph.RenderOptions`). Those types are
// owned by the PURE `rustty-font` leaf BELOW this tier; re-export them here as
// the shared face types this shaper tier builds on.
pub use rustty_font::glyph::RenderOptions;
pub use rustty_font::metrics::Metrics;

// ---------------------------------------------------------------------------
// PORT NOTE: the `main.zig`-level declarations of the `font` module. `main.zig`
// re-exports the sub-files as one namespace and defines the small shared enums,
// the build `options`, and the fallback constants. In rustty the pure leaf
// (`rustty-font`) owns `Atlas`/`Glyph`/`Metrics`/`CodepointMap`/`sprite`/
// `opentype`/`embedded`; this shaper tier owns `face`/`Collection`/`DeferredFace`/
// `CodepointResolver`/`Library`/`discovery`. The `main.zig` glue below wires them
// together (the `font.*` names the ported files reference).
// ---------------------------------------------------------------------------

// pub const Atlas = @import("Atlas.zig");
pub use rustty_font::atlas::Atlas;
// pub const Backend = @import("backend.zig").Backend;
pub use backend::Backend;
// pub const CodepointMap = @import("CodepointMap.zig");
pub use rustty_font::codepoint_map::CodepointMap;
// pub const CodepointResolver = @import("CodepointResolver.zig");
pub use codepoint_resolver::CodepointResolver;
// pub const Collection = @import("Collection.zig");
pub use collection::Collection;
// pub const DeferredFace = @import("DeferredFace.zig");
pub use deferred_face::DeferredFace;
// pub const Glyph = @import("Glyph.zig");
pub use rustty_font::glyph::Glyph;
// pub const Metrics = @import("Metrics.zig"); — re-exported above.
// pub const Descriptor = discovery.Descriptor;
pub use discovery::Descriptor;
// pub const Discover = discovery.Discover;
pub use discovery::Discover;
// pub const Library = library.Library;
pub use library::Library;
// pub const Shaper = shape.Shaper;
pub use shape::Shaper;
// pub const ShaperCache = shape.Cache;
pub use shape::Cache as ShaperCache;
// pub const SharedGrid = @import("SharedGrid.zig");
pub use shared_grid::SharedGrid;
// pub const SharedGridSet = @import("SharedGridSet.zig");
pub use shared_grid_set::SharedGridSet;
// pub const SpriteFace = sprite.Face;
pub use rustty_font::sprite::Face as SpriteFace;
// pub const Sprite = sprite.Sprite;
pub use rustty_font::sprite::Sprite;

/// Build options
// pub const options: struct { backend: Backend } = .{ .backend = ... };
//
// TODO: we need to modify the build config for wasm builds. the issue
// is we're sharing the build config options between all exes in build.zig.
// We need to construct it per target.
//
// PORT NOTE: the reference `options` is an anonymous struct with one `backend`
// field, set from `build_config.font_backend` (or `.web_canvas` for wasm). Here
// it is a module holding the `BACKEND` const. `build_config.font_backend` is the
// `-Dfont-backend` option (`rustty-build-config`'s `font-backend-*` features,
// defaulting to `Backend.default`); `build.rs` receives it as that crate's
// `links` metadata and emits the `rustty_font_backend` cfg read here, the same
// cfg every type selector in this crate (`Face`, `Shaper`, `Library`,
// `Discover`, the `DeferredFace` payloads) is keyed on — so the value and the
// selected types can never disagree.
// (N266 CLOSED by S-14: this used to be `Backend::default()`, folding the
// option to the per-OS default.)
pub mod options {
    use crate::backend::Backend;

    /// The active font backend for this build.
    // .backend = if (builtin.target.cpu.arch.isWasm()) .web_canvas else build_config.font_backend,
    pub const BACKEND: Backend = if cfg!(rustty_font_backend = "freetype") {
        Backend::Freetype
    } else if cfg!(rustty_font_backend = "freetype_windows") {
        Backend::FreetypeWindows
    } else if cfg!(rustty_font_backend = "fontconfig_freetype") {
        Backend::FontconfigFreetype
    } else if cfg!(rustty_font_backend = "coretext") {
        Backend::Coretext
    } else if cfg!(rustty_font_backend = "coretext_freetype") {
        Backend::CoretextFreetype
    } else if cfg!(rustty_font_backend = "coretext_harfbuzz") {
        Backend::CoretextHarfbuzz
    } else if cfg!(rustty_font_backend = "coretext_noshape") {
        Backend::CoretextNoshape
    } else if cfg!(rustty_font_backend = "web_canvas") {
        Backend::WebCanvas
    } else {
        panic!("rustty-font-shaper: build.rs emitted no `rustty_font_backend` cfg")
    };

    /// `build_config.font_backend` as this crate's `Backend` (the two enums
    /// are the same `backend.zig` type in the reference; the port defines it
    /// twice, BREAK #3).
    pub const fn build_config_backend() -> Backend {
        use rustty_build_config::font::Backend as B;
        match rustty_build_config::font_backend() {
            B::Freetype => Backend::Freetype,
            B::FreetypeWindows => Backend::FreetypeWindows,
            B::FontconfigFreetype => Backend::FontconfigFreetype,
            B::Coretext => Backend::Coretext,
            B::CoretextFreetype => Backend::CoretextFreetype,
            B::CoretextHarfbuzz => Backend::CoretextHarfbuzz,
            B::CoretextNoshape => Backend::CoretextNoshape,
            B::WebCanvas => Backend::WebCanvas,
        }
    }

    // `build_config.font_backend` and `options.backend` must be one value off
    // wasm (`main.zig:50`). The cfg comes from build-config's build script and
    // the const from its features; this proves the two channels agree.
    #[cfg(not(target_arch = "wasm32"))]
    const _: () = assert!(
        BACKEND as u8 == build_config_backend() as u8,
        "font options.backend disagrees with build_config.font_backend"
    );
}

/// RUSTTY-ONLY (S-14, N266): the backend selectors (`face.zig`, `shape.zig`,
/// `library.zig`, `discovery.zig`) are cfg-keyed type aliases; this restates each
/// reference `switch (options.backend)` against the runtime `options::BACKEND`
/// and checks the alias the build actually resolved. `RUSTTY_EXPECT_FONT_BACKEND`
/// (set by the G-FONTBACKEND lane for every backend it builds) must then name
/// `@tagName(options.backend)`, so a lane whose `-Dfont-backend` feature did not
/// reach the build fails instead of re-testing the default.
#[cfg(test)]
mod backend_selection_tests {
    use crate::{options, Backend};
    use core::any::type_name;

    fn tag(b: Backend) -> &'static str {
        match b {
            Backend::Freetype => "freetype",
            Backend::FreetypeWindows => "freetype_windows",
            Backend::FontconfigFreetype => "fontconfig_freetype",
            Backend::Coretext => "coretext",
            Backend::CoretextFreetype => "coretext_freetype",
            Backend::CoretextHarfbuzz => "coretext_harfbuzz",
            Backend::CoretextNoshape => "coretext_noshape",
            Backend::WebCanvas => "web_canvas",
        }
    }

    #[test]
    fn selectors_follow_options_backend() {
        use Backend::*;
        let b = options::BACKEND;

        if let Ok(want) = std::env::var("RUSTTY_EXPECT_FONT_BACKEND") {
            assert_eq!(want, tag(b), "the lane's -Dfont-backend reached the build");
        }
        assert_eq!(b, options::build_config_backend());
        // No `-Dfont-backend`: `Backend.default` (this crate's `backend.zig`
        // port), which build-config's build script applies the same way.
        if rustty_build_config::build::FONT_BACKEND_OPTION.is_none() {
            assert_eq!(b, Backend::default(), "the per-target default");
        } else {
            assert_eq!(
                Some(b as u8),
                rustty_build_config::build::FONT_BACKEND_OPTION.map(|o| o as u8),
                "the option's value"
            );
        }

        // face.zig `Face`
        let face = match b {
            Freetype | FreetypeWindows | FontconfigFreetype | CoretextFreetype => {
                "face::freetype::Face"
            }
            Coretext | CoretextHarfbuzz | CoretextNoshape => "face::coretext::Face",
            WebCanvas => "face::web_canvas::Face",
        };
        assert!(
            type_name::<crate::Face>().ends_with(face),
            "{}",
            type_name::<crate::Face>()
        );

        // shape.zig `Shaper`
        let shaper = match b {
            Freetype | FreetypeWindows | FontconfigFreetype | CoretextFreetype
            | CoretextHarfbuzz => "harfbuzz::Shaper",
            Coretext => "coretext::Shaper",
            CoretextNoshape => "noop::Shaper",
            WebCanvas => "web_canvas::Shaper",
        };
        assert!(
            type_name::<crate::Shaper>().ends_with(shaper),
            "{}",
            type_name::<crate::Shaper>()
        );

        // library.zig `Library`
        let library = match b {
            Freetype | FreetypeWindows | FontconfigFreetype | CoretextFreetype => "FreetypeLibrary",
            Coretext | CoretextHarfbuzz | CoretextNoshape | WebCanvas => "NoopLibrary",
        };
        assert!(
            type_name::<crate::Library>().ends_with(library),
            "{}",
            type_name::<crate::Library>()
        );

        // discovery.zig `Discover` (`void` arms: the port's no-op discoverer)
        let discover = match b {
            Freetype => "NoDiscovery",
            WebCanvas => "NoDiscovery",
            FreetypeWindows => "Windows",
            FontconfigFreetype => "Fontconfig",
            Coretext | CoretextFreetype | CoretextHarfbuzz | CoretextNoshape => "CoreText",
        };
        assert!(
            type_name::<crate::Discover>().ends_with(discover),
            "{}",
            type_name::<crate::Discover>()
        );
        assert_eq!(
            matches!(b, Freetype | WebCanvas),
            crate::discovery::DISCOVER_IS_VOID,
            "Discover == void"
        );
    }
}

/// The styles that a family can take.
// pub const Style = enum(u3) { regular=0, bold=1, italic=2, bold_italic=3 };
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Style {
    Regular = 0,
    Bold = 1,
    Italic = 2,
    BoldItalic = 3,
}

/// The presentation for an emoji.
// pub const Presentation = enum(u1) { text=0 /*U+FE0E*/, emoji=1 /*U+FE0F*/ };
// PORT NOTE: `Hash` is derived (beyond the reference's plain enum) so
// `Presentation` can be a field of `SharedGrid`'s `CodepointKey` hash-map key
// (the reference's `AutoHashMapUnmanaged` auto-derives key hashing).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Presentation {
    /// U+FE0E
    Text = 0,
    /// U+FE0F
    Emoji = 1,
}

/// A FontIndex that can be used to use the sprite font directly.
// pub const sprite_index = Collection.Index.initSpecial(.sprite);
pub const SPRITE_INDEX: collection::Index =
    collection::Index::init_special(collection::index::Special::Sprite);

/// The default font size adjustment we use when loading fallback fonts.
///
/// TODO: Add user configuration for this instead of hard-coding it.
// pub const default_fallback_adjustment: Collection.SizeAdjustment = .ic_width;
pub const DEFAULT_FALLBACK_ADJUSTMENT: collection::SizeAdjustment =
    collection::SizeAdjustment::IcWidth;

// If we're targeting wasm then we export some wasm APIs.
// comptime {
//     if (builtin.target.cpu.arch.isWasm()) {
//         _ = Atlas.Wasm;
//         _ = DeferredFace.Wasm;
//         _ = face.web_canvas.Wasm;
//         _ = shape.web_canvas.Wasm;
//     }
// }
// PORT NOTE: Rust has no comptime decl-retention; the four `Wasm` export sets
// are `#[cfg(target_arch = "wasm32")]` `extern "C"` items in their own
// modules (`rustty_font::atlas`, `deferred_face`, `face::web_canvas`,
// `shaper::web_canvas`), and the wasm cdylib artifacts retain them.

// test {
//     // For non-wasm we want to test everything we can
//     if (!comptime builtin.target.cpu.arch.isWasm()) {
//         @import("std").testing.refAllDecls(@This());
//         return;
//     }
//
//     _ = Atlas;
// }
// PORT NOTE: `refAllDecls` has no Rust analogue; every module's `#[cfg(test)]`
// tests are compiled by `cargo test` regardless of which items it references.
