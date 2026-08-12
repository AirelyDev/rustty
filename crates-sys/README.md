# FFI System Crates

This folder contains the `*-sys` FFI crates that bind Rustty to the C
libraries it depends on. Each crate here vendors the upstream C sources
under its own `vendor/` directory, exposes the generated Rust bindings
under `src/`, and drives the native build from `build.rs`.

These are the Rust successors to the vendored-C packages that lived in
Ghostty's `pkg/` folder: where upstream kept hand-written Zig wrappers
around each library, Rustty keeps a Cargo `*-sys` crate that vendors the
same C and links it. The crates are kept in-tree with Rustty for the same
reason the original packages were: to avoid maintaining them as separate
projects (dedicated issues, PRs, etc.). If you want to reuse one, you can
copy the crate directory into your project.

The current crates are:

- `breakpad-sys`
- `dcimgui-sys`
- `fontconfig-sys`
- `freetype-sys`
- `gettext-sys`
- `glslang-sys`
- `harfbuzz-sys`
- `highway-sys`
- `libpng-sys`
- `oniguruma-sys`
- `sentry-sys`
- `simdutf-sys`
- `spirv-cross-sys`
- `wuffs-sys`
- `zlib-sys`

## License

**This license only applies to the Rust wrapper code within the
`crates-sys` folder of the Rustty project — the `Cargo.toml`, `build.rs`,
`wrapper.h`, and generated `src/` bindings authored for Rustty. It does
not apply to the rest of the Rustty project, nor to the vendored C sources
under each crate's `vendor/` directory: those retain their own upstream
licenses (see the `LICENSE`, `COPYING`, `AUTHORS`, etc. shipped inside each
`vendor/`). The wrapper code descends from Ghostty's `pkg/` packages.**

Copyright © 2024 Mitchell Hashimoto, Ghostty contributors

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the “Software”), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
of the Software, and to permit persons to whom the Software is furnished to do
so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
