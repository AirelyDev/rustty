# Renderer custom-shader (shadertoy) path — design

Fills the DEFERRED custom-shader path in `rustty-renderer`: un-defer the
`RepeatablePath` config type, port `shadertoy::load_from_files` /
`load_from_file`, and wire `init_shaders` to actually load + transpile + build
the post-process pipelines so the ALREADY-WIRED `draw_frame` shadertoy passes
activate.

REFERENCE (read-only): `/Users/gael/projects/ghostty` — `src/renderer/shadertoy.zig`
(`loadFromFiles`/`loadFromFile`), `src/renderer/generic.zig`
(`init_shaders`, `has_custom_shaders`), `src/renderer/Metal.zig`
(`custom_shader_target = .msl`).

Rules: PORT_RULES.md — R1 (100% fidelity; byte-slices → `&[u8]`/`Vec<u8>`/`CString`,
NEVER lossy `String` for shader sources + paths), R4 (ACYCLIC — renderer gains NO
config edge), R5 (deferred = `unimplemented!()` seam + verbatim body).

---

## 0. Feasibility — all three gates GREEN

**feasible = feasible.** The three preconditions the ARCHITECT brief flagged are
each confirmed against the tree:

1. **Transpile chain reusable.** `crate::shadertoy` exports the full chain as
   public fns with byte-faithful signatures (shadertoy.rs):
   - `glsl_from_shader(writer: &mut Vec<u8>, src: &[u8])` (:237)
   - `spirv_from_glsl(writer: &mut Vec<u8>, errlog: Option<&mut SpirvLog>, src: &[u8]) -> Result<(), Error>` (:254)
   - `msl_from_spv(spv: &[u8]) -> Result<Vec<u8>, Error>` (:432)
   - `glsl_from_spv(spv: &[u8]) -> Result<Vec<u8>, Error>` (:462)
   - `pub enum Target { Glsl, Msl }` (:100), `pub enum Error { …, FileNotFound, StreamTooLong, Io }` (:113 — the load-path error variants ALREADY exist, reserved for exactly this milestone), `pub struct SpirvLog` (:383).
   The 4 `test{}` blocks (`spirv`, `spirv_invalid`, `shadertoy_to_msl`,
   `shadertoy_to_glsl`) already exercise the chain end-to-end. `load_from_file`
   is a thin composition of these — nothing new in the FFI is required.

2. **`draw_frame` post-process passes wired.** generic.rs `draw_frame` already
   contains the complete shadertoy post-process, DEAD only because
   `has_custom_shaders == false`:
   - lazy `CustomShaderState::init` + `resize` when `has_custom_shaders && frame.custom_shader_state.is_none()` (:5074–5090); tears it down when `has_custom_shaders` goes false (:5095–5098).
   - the cell render pass targets `state.back_texture` when a custom-shader state exists, else `frame.target` (:5299–5302).
   - the ping-pong post loop: sync uniforms, then `for (i, pipeline) in post_pipelines` render `back_texture` → (`front_texture` if `i < n-1` else `frame.target`) + `state.swap()` (:5445–5501).
   `CustomShaderState::{init,resize,swap}` (:1682–1706), `FrameState` resize
   forwarding (:1638–1640), and `SwapChain::init(api, custom_shaders)`
   (:1423) are all implemented. `Metal::CUSTOM_SHADER_TARGET = Target::Msl`
   exists (metal_impl.rs:142). Activation therefore = flip `has_custom_shaders`
   true + populate `post_pipelines` — both flow from `init_shaders`.

3. **Acyclic `RepeatablePath` reproducible.** `rustty-renderer/Cargo.toml` does
   NOT depend on `rustty-config` (only a comment noting the cycle); `rustty-config`
   DOES depend on `rustty-renderer` (Cargo.toml:48). So the exact bg-image pattern
   applies: a renderer-local `RepeatablePath` (a `Vec<Path>` over the JUST-LANDED
   renderer-local `Path` enum), a `RendererConfig::custom_shaders()` accessor added
   to the trait IN the renderer, impl-d IN `rustty-config`. No `renderer → config`
   edge is created; `check_acyclic.sh` (`.port/tools/gates/check_acyclic.sh`) stays
   green.

No blocker. This doc specifies the full fill.

---

## 1. Renderer-local `RepeatablePath` (un-defer the placeholder)

Reference `configpkg.RepeatablePath` = `struct { value: std.ArrayListUnmanaged(Path) }`
(config/path.zig:386). The renderer-local mirror replaces the current inhabited-ZST
placeholder (generic.rs:250) with a real list over the renderer-local `Path`
(generic.rs:141, already promoted by the bg-image milestone — a byte-faithful
`CString` carrier with `Optional`/`Required` arms).

```rust
// crates/rustty-renderer/src/generic.rs — REPLACES the ZST placeholder at :250.
//
// PORT: reference `configpkg.RepeatablePath` (config/path.zig). Renderer-local
// (R4/G7 — the renderer may not name `rustty_config::path::RepeatablePath`; config
// already deps renderer). `Config` maps its own `RepeatablePath` into this via
// `RendererConfig::custom_shaders` (config-side, acyclic — the exact bg-image
// `Path` pattern). Byte-faithful: each element is the renderer-local `Path`
// (`CString`, never lossy `String`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RepeatablePath {
    // value: std.ArrayListUnmanaged(Path)
    pub value: Vec<Path>,
}

impl RepeatablePath {
    /// Reference `RepeatablePath.equal` (`len` then per-element `a.equal(b)`).
    /// Derived `PartialEq` on `Vec<Path>` is exactly this; kept for R1 parity
    /// and named-call fidelity (`change_config` uses `!=`, generic.rs:2117).
    pub fn equal(&self, other: &RepeatablePath) -> bool {
        self == other
    }
}
```

Notes:
- `change_config` already change-detects with `self.config.custom_shaders != config.custom_shaders` (:2117). The derived `PartialEq` over `Vec<Path>` reproduces the reference `equal` (length + per-element) unchanged — no edit at that site beyond dropping the "placeholder" wording in the adjacent comment.
- The reference `clone`/`expand`/`parseCLI`/`formatEntry` on `RepeatablePath` live in `rustty-config` (config/path.rs:389 — already fully ported). The renderer-local mirror needs only construction + `equal`; the "clone" is subsumed by owned `CString`/`Vec` + the accessor's `.collect()`.
- Remove the "`Renderer::init` reads `custom_shaders.value.items.len` → one reason init is deferred" note at :247 (still true that `init` is a seam, but `.value` now exists).

### 1.1 `RendererConfig::custom_shaders()` — trait method (renderer)

Add ONE method to the `RendererConfig` trait (config.rs, alongside `bg_image` at :100):

```rust
// crates/rustty-renderer/src/config.rs — in `pub trait RendererConfig`.
//
// custom-shader: config `RepeatablePath` -> the renderer-local [`RepeatablePath`]
// (§1). Each element's optional/required arm + null-terminated path bytes are
// preserved byte-faithfully (`CString`, never lossy `String`). Replaces the
// reference's `config.@"custom-shader".clone(alloc)` in `DerivedConfig::init`
// (routed through the accessor because the renderer may not name
// `rustty_config::path::RepeatablePath`, R4/G7).
fn custom_shaders(&self) -> RepeatablePath;
```

(`RepeatablePath` is already `use crate::generic::{…}` in config.rs — extend the
existing import at :27 to include `RepeatablePath`.)

### 1.2 `impl RendererConfig for Config` — accessor body (rustty-config, acyclic)

Mirror `bg_image` (renderer_config.rs:159) exactly, lifted to a list:

```rust
// crates/rustty-config/src/renderer_config.rs — in `impl RendererConfig for Config`.
fn custom_shaders(&self) -> rustty_renderer::generic::RepeatablePath {
    use rustty_renderer::generic::{Path as RPath, RepeatablePath as RRepeatablePath};
    // config `self.custom_shader` is `RepeatablePath { value: Vec<crate::path::Path> }`
    // (path.rs). Map each arm into the renderer-local `Path` (a `CString` over the
    // same bytes). The parsed bytes carry no interior NUL (`Path::parse` dupeZ'd
    // them), so `CString::new` cannot fail on well-formed config; map defensively
    // (an interior NUL => drop that entry, matching the reference "path won't open"
    // degradation). Byte-faithful (never a lossy `String`). This IS the reference
    // `config.@"custom-shader".clone(alloc)` — the per-element clone is the collect.
    let value = self
        .custom_shader
        .value
        .iter()
        .filter_map(|p| match p {
            crate::path::Path::Optional(b) => CString::new(b.clone()).ok().map(RPath::Optional),
            crate::path::Path::Required(b) => CString::new(b.clone()).ok().map(RPath::Required),
        })
        .collect();
    RRepeatablePath { value }
}
```

`CString` and `crate::path::Path` are already imported in renderer_config.rs
(:22, :169–171). No new crate edge.

### 1.3 Un-defer `DerivedConfig::init` custom_shaders extraction

In `DerivedConfig::init<C>` (generic.rs), replace the placeholder assignment
(:435–439):

```rust
// BEFORE (placeholder):
//   let custom_shaders = RepeatablePath;
// AFTER — reference `const custom_shaders = try config.@"custom-shader".clone(alloc);`
// The accessor's per-element `CString` clone + `.collect()` IS the reference clone.
let custom_shaders = config.custom_shaders();
```

The `DerivedConfig` struct field type (`pub custom_shaders: RepeatablePath`, :307)
is unchanged — it now carries the real list. Update the field comment at :278 to
drop "inhabited placeholder / the fill never reads it" (it is now read by
`init_shaders`).

---

## 2. `shadertoy::load_from_files` / `load_from_file` (un-defer the seam)

Restore the two functions in shadertoy.rs (currently the commented verbatim block
at :124–224). They compose the ALREADY-ported chain; nothing new in FFI. The
reference arena (`ArenaAllocator` for `src`/`glsl`/`spirv`, `alloc_gpa` for the
returned source) collapses to local `Vec`s (dropped at scope end) + an owned
`CString` return.

```rust
// crates/rustty-renderer/src/shadertoy.rs — replaces the DEFERRED comment block.
use std::ffi::{CStr, CString};
use crate::generic::{Path, RepeatablePath};

/// Load a set of shaders from files and convert them to the target
/// format. The shader order is preserved.
// pub fn loadFromFiles(alloc_gpa, paths: configpkg.RepeatablePath, target) ![]const [:0]const u8
//
// PORT NOTE: `paths: configpkg.RepeatablePath` -> the renderer-local
// `&RepeatablePath` (§1). Returns `Vec<CString>` (the reference `[]const [:0]const
// u8` — owned, NUL-terminated shader sources; byte-faithful, never lossy `String`).
// The reference `alloc_gpa` + `errdefer free(shader)` dissolve into `Vec`/`CString`
// Drop (a mid-loop error drops the already-built sources automatically).
pub fn load_from_files(paths: &RepeatablePath, target: Target) -> Result<Vec<CString>, Error> {
    // var list: std.ArrayList([:0]const u8) = .empty;
    let mut list: Vec<CString> = Vec::new();

    // for (paths.value.items) |item| {
    for item in &paths.value {
        // const path, const optional = switch (item) {
        //     .optional => |path| .{ path, true },
        //     .required => |path| .{ path, false },
        // };
        let (path, optional): (&CStr, bool) = match item {
            Path::Optional(p) => (p.as_c_str(), true),
            Path::Required(p) => (p.as_c_str(), false),
        };

        // const shader = loadFromFile(alloc_gpa, path, target) catch |err| {
        //     if (err == error.FileNotFound and optional) { continue; }
        //     return err;
        // };
        let shader = match load_from_file(path, target) {
            Ok(s) => s,
            Err(Error::FileNotFound) if optional => continue,
            Err(err) => return Err(err),
        };
        // log.info("loaded custom shader path={s}", .{path});   // no logging facade (comment)
        // try list.append(alloc_gpa, shader);
        list.push(shader);
    }

    // return try list.toOwnedSlice(alloc_gpa);
    Ok(list)
}

/// Load a single shader from a file and convert it to the target language
/// ready to be used with renderers.
// pub fn loadFromFile(alloc_gpa, path: []const u8, target) ![:0]const u8
//
// PORT NOTE: `path: []const u8` -> `&CStr` (the renderer-local `Path` carries a
// `CString`; the load reads the path bytes). Returns an owned NUL-terminated
// `CString` (reference `[:0]const u8`). The reference arena intermediates
// (`src`/`glsl`/`spirv`) become local `Vec`s.
pub fn load_from_file(path: &CStr, target: Target) -> Result<CString, Error> {
    // Read it all into memory -- we don't expect shaders to be large.
    // const src = ... file.readToEndAlloc(alloc, 4 * 1024 * 1024);  // 4MB cap
    //
    // PORT NOTE: `std.fs.cwd().openFile(path)` over the raw path bytes. On unix the
    // path bytes map to `OsStr` via `OsStrExt::from_bytes` (byte-faithful — the
    // reference opens the exact bytes, never a lossy `String`). `error.FileNotFound`
    // -> `Error::FileNotFound` (drives the optional-arm skip in `load_from_files`);
    // other I/O -> `Error::Io`; exceeding the 4MB cap -> `Error::StreamTooLong`.
    let src: Vec<u8> = read_shader_file(path)?;

    // Convert to full GLSL (+ NUL sentinel, matching the reference `writeByte(0)`).
    // var stream ...; try glslFromShader(&stream.writer, src); try writeByte(0);
    let mut glsl: Vec<u8> = Vec::new();
    glsl_from_shader(&mut glsl, &src);
    glsl.push(0);

    // Convert to SPIR-V.
    // var errlog: SpirvLog = .{...}; spirvFromGlsl(&stream.writer, &errlog, glsl) catch |err| {
    //     if (errlog.info.len > 0 or errlog.debug.len > 0) log.warn(...);  // comment
    //     return err;
    // };
    let mut spirv: Vec<u8> = Vec::new();
    let mut errlog = SpirvLog::default();
    if let Err(err) = spirv_from_glsl(&mut spirv, Some(&mut errlog), &glsl) {
        // log.warn("spirv error path={s} info={s} debug={s}", ...);   // no facade
        let _ = &errlog; // (info/debug observed only by the elided log.warn)
        return Err(err);
    }
    // (The reference re-packs into a u32-aligned list for the `@alignCast`; the
    // port's `spv_cross` already copies into a `Vec<u32>` internally, so `spirv`
    // is passed directly — same subsumption the `shadertoy_to_msl` test documents.)

    // Convert to MSL / GLSL per target.
    // return switch (target) { .glsl => glslFromSpv(alloc_gpa, spirv), .msl => mslFromSpv(...) };
    let out: Vec<u8> = match target {
        Target::Glsl => glsl_from_spv(&spirv)?,
        Target::Msl => msl_from_spv(&spirv)?,
    };
    // The transpiled source is C-string text (no interior NUL — `c_str_to_vec`
    // stops at the first NUL), so `CString::new` cannot fail; map defensively.
    CString::new(out).map_err(|_| Error::SpvcFailed)
}

// PORT NOTE (helper): the reference's `openFile` + `readToEndAlloc(alloc, 4MB)`.
// Byte-faithful path handling (unix `OsStrExt::from_bytes`); 4MB cap -> StreamTooLong.
fn read_shader_file(path: &CStr) -> Result<Vec<u8>, Error> {
    use std::io::Read;
    use std::os::unix::ffi::OsStrExt;
    let os = std::ffi::OsStr::from_bytes(path.to_bytes());
    let mut file = std::fs::File::open(os).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::FileNotFound,
        _ => Error::Io,
    })?;
    // readToEndAlloc(alloc, 4 * 1024 * 1024) — cap read at 4MB, else StreamTooLong.
    const MAX: u64 = 4 * 1024 * 1024;
    let mut buf = Vec::new();
    file.by_ref()
        .take(MAX + 1)
        .read_to_end(&mut buf)
        .map_err(|_| Error::Io)?;
    if buf.len() as u64 > MAX {
        return Err(Error::StreamTooLong);
    }
    Ok(buf)
}
```

Notes / fidelity:
- `SpirvLog`, `Target`, `Error::{FileNotFound,StreamTooLong,Io,SpvcFailed}`, and the four transpile fns are all already present — this block introduces no new types.
- The commented DEFERRED-block header at shadertoy.rs:11–18 + :124–128 (module-doc "DEFERRED: loadFromFiles/loadFromFile") must be updated to "ported" and the R5 verbatim comment block removed (now live code).
- `read_shader_file` is unix-only (`OsStrExt`). The renderer is macOS/Metal-gated already; keep the module cross-compilable by not gating (unix covers macOS + the Linux host that runs the `load_from_files` host test). If a non-unix target is ever added, gate then.

### 2.1 In-scope R1 fix: lossless `NSString` at the Metal source boundary

This milestone is the FIRST to push real shader-source bytes through
`init_post_pipeline` (`post_pipelines` was always empty before), so it activates a
lossy-`String` step on exactly the data R1 names ("shader sources … NEVER lossy
`String`"). `init_post_pipeline` currently constructs the Metal library source via
`NSString::from_str(&data.to_string_lossy())` (metal/shaders.rs:499, verified) — a
lossy conversion that would substitute U+FFFD on any non-UTF-8 byte. spirv-cross MSL
is ASCII so it is behaviorally sound (no render break), but to honor R1's letter the
`NSString` must be built from the raw `CStr` bytes losslessly:

```rust
// crates/rustty-renderer/src/metal/shaders.rs — in `init_post_pipeline`, REPLACES
// `let src = NSString::from_str(&data.to_string_lossy());`.
//
// R1 (byte-faithful shader sources): the transpiled MSL is C-string text; decode
// its bytes as UTF-8 WITHOUT lossy replacement. spirv-cross emits ASCII so this
// never fails in practice, but an invalid byte is surfaced as an error rather than
// silently corrupted (the reference passes the exact `[:0]const u8` bytes).
let text = std::str::from_utf8(data.to_bytes()).map_err(|_| Error::MetalFailed)?;
let src = NSString::from_str(text);
```

This is a one-line, non-behavior-breaking source edit (hence a fix, not a blocker).
It elevates OPEN RISK R2 from "out of scope" to a landed fix — see §6 R2. It adds
`crates/rustty-renderer/src/metal/shaders.rs` to the touched-file set (§5).

---

## 3. `init_shaders` wiring (activate the dead post-process)

Replace the DEFERRED seam in `init_shaders` (generic.rs:4749–4784). The reference
body (currently preserved as comments at :4750–4762) becomes live:

```rust
fn init_shaders(&mut self) -> Result<(), Error> {
    // Load our custom shaders.
    // const custom_shaders = shadertoy.loadFromFiles(
    //     arena_alloc, self.config.custom_shaders, GraphicsAPI.custom_shader_target,
    // ) catch |err| err: { log.warn(...); break :err &.{}; };
    //
    // The reference arena is subsumed by owned `Vec<CString>` (dropped after
    // `init_shaders` returns — the sources are copied into `NSString` by
    // `init_post_pipeline`). The `catch |err| break :err &.{}` (any load error =>
    // no custom shaders, never block the renderer) maps to `unwrap_or_default()`.
    let custom_shaders: Vec<CString> = shadertoy::load_from_files(
        &self.config.custom_shaders,
        Metal::CUSTOM_SHADER_TARGET,
    )
    .unwrap_or_else(|_err| {
        // log.warn("error loading custom shaders err={}", .{err});   // no facade
        Vec::new()
    });

    // GraphicsAPI.initShaders takes `[]const [:0]const u8`; build the borrow slice.
    let custom_refs: Vec<&CStr> = custom_shaders.iter().map(|c| c.as_c_str()).collect();

    // const has_custom_shaders = custom_shaders.len > 0;   // from the LOADED list.
    let has_custom_shaders = !custom_refs.is_empty();

    // var shaders = try self.api.initShaders(self.alloc, custom_shaders);
    self.shaders = self.api.init_shaders(&custom_refs)?;
    // self.has_custom_shaders = has_custom_shaders;
    self.has_custom_shaders = has_custom_shaders;
    Ok(())
}
```

This flows straight into the already-wired downstream (§0.2):
- `Metal::init_shaders(&custom_refs)` → `Shaders::init` → `init_post_pipelines` compiles each MSL source into a `Pipeline`, populating `self.shaders.post_pipelines` (shaders.rs:373, :457–482). Empty input ⇒ empty `post_pipelines` (unchanged golden path).
- `has_custom_shaders = true` unlocks the `draw_frame` branch that lazily builds `CustomShaderState`, retargets the cell pass to `back_texture`, and runs the ping-pong post loop.
- `has_animations()` (:4745) already returns `self.has_custom_shaders`; no change.

`init_shaders`'s only caller today is `draw_frame`'s reinit-on-defunct path
(:5059); it is also the natural call from the (still-deferred) `Renderer::init`.
The tests (§4) call `init_shaders()` directly on a field-built renderer — the same
way the golden bg-image test drives `prep_background_image()`.

---

## 4. Tests

Three tests, matching the ARCHITECT brief. G3b comment-coverage SKIPS
`rustty-renderer`, so these tests + the adversary/verify are the only R1 backstop
for the fill.

A **pass-through shadertoy shader** (identity blit) is the fixture. The prefix
(`shaders/shadertoy_prefix.glsl`) already declares `iChannel0` (sampler2D binding 0)
and `iResolution`; the body is:

```glsl
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    fragColor = texture(iChannel0, fragCoord / iResolution.xy);
}
```

`init_post_pipeline` binds `iChannel0` ← `state.back_texture` (where the cells were
just rendered) and blits it 1:1 to `frame.target` — identity.

### 4.1 Host-only: `load_from_files` transpiles a GLSL file (cross-platform)

In shadertoy.rs `mod tests` (no Metal — glslang + spirv-cross are host libs;
target `Glsl` so no Metal device is touched):

```rust
#[test]
fn load_from_files_transpiles_glsl() {
    use std::ffi::CString;
    // Write the pass-through shadertoy body to a nanos-suffixed temp file
    // (mirrors the bg-image golden, generic.rs:6116-6121).
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("rustty_shader_{nanos}.glsl"));
    std::fs::write(&path, b"void mainImage(out vec4 fragColor, in vec2 fragCoord){\n\
        fragColor = texture(iChannel0, fragCoord / iResolution.xy);\n}\n").unwrap();

    // Byte-faithful path -> CString (mirrors the bg-image golden, generic.rs:6065:
    // `CString::new(p.as_os_str().as_encoded_bytes().to_vec())`).
    let cpath = CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap();
    let rp = RepeatablePath {
        value: vec![Path::Required(cpath)],
    };
    let out = load_from_files(&rp, Target::Glsl).unwrap();
    assert_eq!(out.len(), 1);
    assert!(!out[0].as_bytes().is_empty(), "transpiled source is empty");

    // Optional-arm missing file is SKIPPED (not an error).
    let rp_missing_opt = RepeatablePath {
        value: vec![Path::Optional(CString::new("/nonexistent/rustty_missing.glsl").unwrap())],
    };
    assert!(load_from_files(&rp_missing_opt, Target::Glsl).unwrap().is_empty());

    // Required-arm missing file is a FileNotFound error.
    let rp_missing_req = RepeatablePath {
        value: vec![Path::Required(CString::new("/nonexistent/rustty_missing.glsl").unwrap())],
    };
    assert!(matches!(load_from_files(&rp_missing_req, Target::Glsl), Err(Error::FileNotFound)));

    let _ = std::fs::remove_file(&path);
}
```

This exercises the entire load/transpile composition AND both control-flow arms
(optional-skip, required-error) with zero Metal dependency — it runs on the Linux
CI host too.

### 4.2 Metal-gated offscreen: end-to-end custom-shader draw (macOS, Metal-skip)

In generic.rs `mod tests`, mirroring `golden_renders_background_image` (:6052) and
`test_renderer` (:5628):

```rust
#[test]
fn golden_renders_custom_shader() {
    use std::ffi::CString;
    let mut r = match test_renderer(10, 3) { Some(r) => r, None => return }; // Metal-skip

    // Write the pass-through shadertoy body to a nanos-suffixed temp file
    // (mirrors the bg-image golden, generic.rs:6116-6121).
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("rustty_shader_{nanos}.glsl"));
    std::fs::write(&path, b"void mainImage(out vec4 fragColor, in vec2 fragCoord){\n\
        fragColor = texture(iChannel0, fragCoord / iResolution.xy);\n}\n").unwrap();

    // Point the config at the on-disk pass-through shader (required arm), byte-faithful
    // path -> CString (mirrors the bg-image golden, generic.rs:6065).
    let cpath = CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap();
    r.config.custom_shaders = RepeatablePath {
        value: vec![Path::Required(cpath)],
    };

    // Load + transpile + build the post pipelines + flip has_custom_shaders.
    r.init_shaders().expect("init_shaders");
    assert!(r.has_custom_shaders, "custom shaders not activated");
    assert!(!r.shaders.post_pipelines.is_empty(), "post pipelines not built");

    // Drive one frame with a non-empty terminal (write visible text so the cell
    // pass produces a recognizable, non-black image the pass-through must preserve).
    // `Terminal::print_string` takes `&[u8]` and returns `Result` (rustty-vt
    // terminal.rs:653) — pass a byte-string literal and unwrap.
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
    t.print_string(b"HELLO").unwrap();
    let st = state::State {
        mutex: &mut mutex,
        terminal: &mut t,
        inspector: None,
        preedit: None,
        mouse: state::Mouse::default(),
    };
    r.update_frame(&st, true).unwrap();
    r.draw_frame(true).unwrap();

    // The draw_frame branch lazily created the per-frame CustomShaderState.
    let idx = r.swap_chain.frame_index as usize;
    assert!(r.swap_chain.frames[idx].custom_shader_state.is_some(),
            "custom_shader_state not created by draw_frame");

    // Pass-through = identity: the cells survive to frame.target (not all-black).
    let px = r.swap_chain.frames[idx].target.read_bgra();
    // Robustness assertion (see below): a sampled glyph pixel is non-black AND the
    // frame is not uniformly one color — the post-process ran AND preserved the cells.
    assert!(/* sampled glyph pixel non-black */,
            "cells not visible through the pass-through shader");
    assert!(/* frame not uniformly one color */,
            "frame is a uniform fill (post-process did not preserve the image)");
    t.deinit(Global);
    let _ = std::fs::remove_file(&path);
}
```

Assertion strategy for "recognizable": use the **robustness check** — "a sampled
glyph pixel is non-black AND the frame is not uniformly one color". Prefer this over
an exact baseline diff: the pass-through samples
`texture(iChannel0, gl_FragCoord.xy/iResolution.xy)` through a Metal sampler, so
sub-LSB filtering (and any origin convention) can perturb an exact pixel match while
leaving the image recognizable — an exact/tolerance baseline diff is thus flaky. The
non-black + not-uniform pair still fails the real thing under test: a black frame
(empty `post_pipelines`, the failure mode in OPEN RISK R1) fails the first check, and
a uniform fill fails the second. (A baseline diff against a no-custom-shader render
remains available as a stricter local sanity aid, but is NOT the asserted contract
because of the sampler-filtering flakiness.)

### 4.3 MockConfig unit: `DerivedConfig::init` maps the list (cross-platform)

Mirror `derived_config_maps_bg_image_path` (:733). Add `custom_shaders` to
`MockConfig` (generic.rs:546) + its `RendererConfig` impl (:657 region), then:

```rust
#[test]
fn derived_config_maps_custom_shaders() {
    use std::ffi::CString;
    let mut cfg = MockConfig::default();
    cfg.custom_shaders = vec![Path::Required(CString::new("/x/shader.glsl").unwrap())];
    let d = DerivedConfig::init(Global, &cfg).expect("init");
    assert_eq!(d.custom_shaders.value.len(), 1);
    match &d.custom_shaders.value[0] {
        Path::Required(p) => assert_eq!(p.to_bytes(), b"/x/shader.glsl"),
        _ => panic!("wrong arm"),
    }
    // Default mock => empty list.
    let d0 = DerivedConfig::init(Global, &MockConfig::default()).expect("init");
    assert!(d0.custom_shaders.value.is_empty());
}
```

MockConfig lives in the renderer (can't dep rustty-config); its `custom_shaders()`
returns the renderer-local `RepeatablePath` directly — so this test also proves the
`init<C>` extraction path independent of the config crate. The `derived_config_for_test`
literal (:5607) changes from `custom_shaders: RepeatablePath` to
`custom_shaders: RepeatablePath::default()` (or an explicit empty `value`).

---

## 5. Dead-code, no-regression, acyclic

- **check_acyclic** (`.port/tools/gates/check_acyclic.sh`): green. Renderer gains NO config edge — the new `RendererConfig::custom_shaders` method lives in the renderer trait; its body lives in `rustty-config` (which already deps renderer). Identical shape to the landed `bg_image` accessor.
- **No regression**: the golden (`golden_renders_background_image`) + the existing 49 tests use `custom_shaders: RepeatablePath` (ZST) or `&[]` post shaders. After the change: `RepeatablePath::default()` is an empty `Vec<Path>`, `init_shaders`'s `load_from_files` over an empty list returns `Ok(vec![])` ⇒ `has_custom_shaders = false` ⇒ `post_pipelines` empty ⇒ the `draw_frame` custom branch stays skipped. The golden path is byte-for-byte unchanged. The only literal edits are `RepeatablePath` → `RepeatablePath::default()` at the two struct-literal sites (:5607 test fixture; and `DerivedConfig::init` no longer assigns the ZST).
- **Dead-code**: `RepeatablePath::equal` is `pub` (dead-code exempt like `Path::equal`). `load_from_files`/`load_from_file` gain live callers (`init_shaders` + the host test). `read_shader_file` is called by `load_from_file`.
- **Compile boundary**: the fill spans two crates but compiles as one unit — `cargo build --workspace` is the backstop. rustty-renderer: shadertoy.rs (load fns) + generic.rs (`RepeatablePath`, `init_shaders`, `DerivedConfig::init`, MockConfig, tests) + config.rs (trait method) + metal/shaders.rs (the §2.1 lossless-`NSString` R1 fix). rustty-config: renderer_config.rs (accessor impl). The §4.1 host test compiles + runs cross-platform; §4.2 is `#[cfg(all(test, target_os = "macos"))]`-gated with Metal-skip (the `test_renderer` `None` path).

---

## 6. OPEN RISKS

- **R1 — untested Chunk-4 passes; black-frame failure mode.** The `draw_frame`
  post-process (§0.2) has NEVER executed (golden uses no custom shaders); §4.2 is
  its first exercise. Structural note: `has_custom_shaders` is set independently of
  `post_pipelines` being non-empty (both derive from the loaded list, but
  `init_post_pipelines` swallows a Metal compile error to an EMPTY `post_pipelines`
  via `unwrap_or_else(Vec::new)`, shaders.rs:374). If `has_custom_shaders == true`
  but `post_pipelines` is empty, the cells render into `back_texture` and are NEVER
  blitted to `frame.target` ⇒ BLACK output. This is FAITHFUL to the reference (same
  structure), but it means §4.2 MUST assert `!post_pipelines.is_empty()` (it does)
  so a silently-black frame can't pass. The `i < n-1` indexing (:5472) is safe: the
  loop body is skipped when `post_pipelines` is empty, so `n-1` never underflows.

- **R2 — lossy `String` at the Metal source boundary — FIXED IN-SCOPE (§2.1).**
  `init_post_pipeline` built the `NSString` via `data.to_string_lossy()`
  (shaders.rs:499) — a lossy `String` step. This milestone is the FIRST to flow real
  shader-source bytes through that call (`post_pipelines` was always empty before), so
  it activates the lossy step on exactly the data R1 names. The adversary review
  elevated this from "out of scope" to an in-scope R1 fix: §2.1 replaces the call with
  `std::str::from_utf8(data.to_bytes())` (lossless; error on invalid UTF-8 rather than
  silent U+FFFD substitution). Behaviorally identical for the ASCII MSL spirv-cross
  emits, so not a blocker — a one-line fidelity fix now landed as part of this fill.

- **R3 — `RepeatablePath` equality on the `change_config` path.** `change_config`
  (:2117) now compares real lists via derived `!=` (reference `!equal()`). Confirmed
  equivalent (length + per-element `Path` `PartialEq`, which is variant + `CString`
  bytes). No behavioral change vs. the reference; noted because it moves from a
  trivially-equal ZST to a content comparison — a change in a config that only
  reorders identical paths now (correctly) counts as changed, matching the reference.

- **R4 — `read_shader_file` unix-only.** Uses `OsStrExt::from_bytes` for
  byte-faithful path opening. Fine for macOS + Linux-host CI; a Windows renderer
  target would need a `cfg` split. Deferred (no Windows renderer exists).

- **R5 — 4MB cap semantics.** The reference `readToEndAlloc(alloc, 4MB)` errors
  `StreamTooLong` past 4MB; the port reproduces this with `take(MAX+1)` + a length
  check. A shader at exactly 4MB passes; >4MB is `StreamTooLong`. Byte-faithful to
  the reference cap.
