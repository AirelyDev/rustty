//! rustty-oniguruma — the safe `oni::*` wrapper over the vendored Oniguruma C.
//!
//! PORTS: ghostty pkg/oniguruma/main.zig (+ init/types/regex/region/match_param/
//! errors/testing)
//!
//! PORT STATUS: done. This is the R1 safe wrapper the reference exposes as the
//! `oni` module: it re-exports `Encoding`/`Syntax`/`MatchParam`/`Regex`/`Region`
//! and the `init`/`deinit`/`testing`/`errors` surface, built on the raw FFI of
//! `oniguruma-sys` (R3: the SAME vendored Oniguruma 6.9.9, built standalone).
//!
//! Consumed by `rustty-vt`'s `string_map.rs` (off-by-default `oniguruma`
//! feature) as the frozen `oni::*` API — the public names here MUST match that
//! surface exactly (`Regex`/`Region`/`MatchParam`/`Error`/`Encoding`/`Syntax`/
//! `RegexOptions`/`SearchOptions`/`testing::ensure_init`).

// const initpkg = @import("init.zig");
// const match_param = @import("match_param.zig");
// const regex = @import("regex.zig");
// const region = @import("region.zig");
// const types = @import("types.zig");
// PORT NOTE: the reference imports `init.zig` as `initpkg` to avoid clashing with
// the re-exported `init` function; we mirror that (`#[path]` module `initpkg`).
#[path = "init.rs"]
mod initpkg;
mod match_param;
mod regex;
mod region;
mod types;

// pub const c = @import("c.zig");
// PORT NOTE: the reference re-exports the raw `c` @cImport; the Rust equivalent
// is the `oniguruma-sys` crate itself, so downstream reaches raw symbols there.
pub use oniguruma_sys as c;

// pub const testing = @import("testing.zig");
// pub const errors = @import("errors.zig");
pub mod errors;
pub mod testing;

// pub const init = initpkg.init;
// pub const deinit = initpkg.deinit;
pub use initpkg::{deinit, init};

// pub const Encoding = types.Encoding;
// pub const MatchParam = match_param.MatchParam;
// pub const Regex = regex.Regex;
// pub const Region = region.Region;
// pub const Syntax = types.Syntax;
pub use errors::Error;
pub use match_param::MatchParam;
pub use regex::Regex;
pub use region::Region;
pub use types::{Encoding, Options, RegexOptions, SearchOptions, Syntax};
