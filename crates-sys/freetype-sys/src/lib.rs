//! freetype-sys — FFI bindings to the vendored FreeType font rasterizer (`links = \"freetype\"`).
//!
//! PORT STATUS: stub
//!
//! R3: copies the SAME C library the reference uses and builds it standalone
//! (cc + bindgen). No pure-Rust substitution during parity phases. No C is
//! compiled yet — see build.rs for the vendoring TODO.
