# Core ↔ Renderer config integration — the non-windowing swap

**Task:** wire the real `rustty_renderer` into `rustty-core` as far as the
*non-windowing* surface allows. Design only — this doc specifies the change; it
touches no `crates/`.

**Repo:** `/Users/gael/projects/rustty` (branch `phase-1-parity-gates`).
**Reference (read-only):** `/Users/gael/projects/ghostty` — `src/Surface.zig`,
`src/renderer.zig`, `src/renderer/{generic,Options,Thread}.zig`.
**Rules:** `.port/PORT_RULES.md` — R1 (100 % fidelity), R4 (acyclic — gate
`.port/tools/gates/check_acyclic.sh`), R5 (windowing/xev stay `unimplemented!()`
seams, verbatim body).

---

## 0. Verdict

`non_windowing_slice = DerivedConfig` (a real, meaningful slice exists — it is
**not** `none`).

One renderer_seam placeholder — the GPU renderer's `DerivedConfig` (the
`renderer.Renderer.DerivedConfig` nested config) — swaps to the real
`rustty_renderer::DerivedConfig` **now**. The config-change message path
(`Message::initChangeConfig`) is **already** wired to the real type. Everything
else (`Renderer::init`/`present`/draw, the `Options<A>` carrier struct, the
`Thread<A>` run loop, `RendererError`) stays a documented windowing/xev seam.

### Swap map

| renderer_seam placeholder | disposition | why |
|---|---|---|
| `DerivedConfig` (struct + `init`/`deinit`) | **SWAP → `rustty_renderer::DerivedConfig`** | cross-platform, headless; `init<C: RendererConfig>` runs with `C = rustty_config::Config` — the one type core can bind that the renderer crate cannot |
| `Options<A>.config` field type | **RE-TYPE → `rustty_renderer::DerivedConfig`** | follows the swap; the `Options<A>` *struct* stays (below) |
| `RendererError` | **STAYS** | opaque marker for the *unported* draw/init/Metal error union; the real renderer has no cross-platform equivalent yet |
| `Renderer` + `surfaceInit`/`init`/`finalizeSurfaceInit`/`threadEnter`/`drawFrame`/`deinit` | **STAYS** (verbatim body, `unimplemented!()`) | real `Renderer` is `#[cfg(target_os="macos")]` + `Metal::init(rt_surface = live NSView)` — windowing-blocked (M-2 seam) |
| `Options<A>` (the struct) | **STAYS** | holds the 3 apprt-typed fields (`surface_mailbox`/`rt_surface`/`thread`) the **real** `rustty_renderer::Options` structurally **cannot** hold (apprt-api → renderer edge would cycle) |
| `Thread<A>` (`init`/`threadMain`/`deinit`, run loop) | **STAYS** (verbatim body, `unimplemented!()`) | D6/xev run loop |
| `Message::initChangeConfig` config-change path | **ALREADY REAL** — no work | core already calls `rendererpkg::Message::initChangeConfig` (surface.rs:4356) |

---

## 1. Audit — how core uses each placeholder

Core aliases `use rustty_renderer as rendererpkg;` (surface.rs:153) and already
deps `rustty-renderer` (Cargo.toml:27) **and** `rustty-config`. It binds the
concrete `C = rustty_config::Config` — that is the whole point of this slice.

**The renderer's `DerivedConfig` is constructed directly by core in exactly one
place** — `Surface::init` (surface.rs:1743):

```rust
let renderer_config = match crate::renderer_seam::DerivedConfig::init(alloc, config) {
    Ok(v) => v,
    Err(err) => { derived_config.deinit(); return Err(err.into()); }
};
let mut renderer_impl = match Renderer::init(alloc, crate::renderer_seam::Options {
    config: renderer_config,           // <-- moved into the seam Options
    font_grid, size,
    surface_mailbox: SurfaceMailbox { surface: self_, app: app_mailbox },
    rt_surface,
    thread: unsafe { &raw mut (*self_).renderer_thread },
}) { ... };
```

Everywhere **else** the renderer's `DerivedConfig` is built *inside*
`Message::initChangeConfig` — which core already calls with the real type:

```rust
// surface.rs:4356  (Surface::updateConfig)
let renderer_message = rendererpkg::Message::initChangeConfig(self.alloc, config)?;
```

`InitChangeConfigError` is already a real core `Error` arm (surface.rs:1191,
1253). So the **config-change plane is done**; the only remaining direct
construction of the renderer `DerivedConfig` is the `Surface::init` derivation.

(Note: `DerivedConfig::init` at surface.rs:1610 / 4286 is core's **own**
`Surface.DerivedConfig` — surface.rs:685 — a different type. Untouched.)

### Real target types (all confirmed present)

- `rustty_renderer::DerivedConfig` — re-exported at crate root (lib.rs:130),
  **cross-platform** (not macОS-gated; the `Message::ChangeConfig` payload names
  it). `pub fn init<C: RendererConfig>(alloc_gpa: Global, config: &C) ->
  Result<DerivedConfig, DerivedConfigError>` (generic.rs:431) — the reference
  body is real, reading through the `RendererConfig` accessor trait
  (config.rs). `pub fn deinit(&mut self)` (generic.rs:536) frees `links`'s onig
  handles over `Global`.
- `rustty_renderer::generic::DerivedConfigError` (generic.rs:272) — opaque
  marker (`ArenaAllocator` OOM + config clones + `link::Set::from_config`'s
  `oni::Error`).
- `impl RendererConfig for Config` lives in
  `crates/rustty-config/src/renderer_config.rs` (acyclic — the impl is in the
  type's home crate). It is **already compile-bound today** at surface.rs:4356
  (`updateConfig` calls `initChangeConfig(self.alloc, config)` with `config:
  &Config`, resolving the `RendererConfig` bound to `Config`). What is
  genuinely new here is the **first HEADLESS execution** of that impl against
  the real `Config`: `updateConfig` cannot run headlessly (it needs a
  fully-constructed `Surface` + a live `renderer_thread.mailbox` to push into,
  surface.rs:4371), whereas the T1/T2 unit tests (§4) call `DerivedConfig::init`
  / `initChangeConfig` directly with no `Surface`. (The renderer crate can only
  test `init<C>` with `test_support::MockConfig` — never the real `Config`.)
- `rustty_renderer::Renderer` — `#[cfg(target_os="macos")] pub use
  generic::Renderer;` (lib.rs:121-122). Names `crate::metal::Metal`; `init`
  needs `Metal::init(rt_surface)` = a live NSView/IOSurfaceLayer. **Windowing —
  stays a seam.**
- `rustty_renderer::Options` (generic.rs:752) — has only `config`/`font_grid`/
  `size`; the 3 apprt-typed fields are **deferred** because
  `rustty-apprt-api → rustty-renderer` (naming an apprt type here would cycle).
  Core's `Options<A>` needs all 6 fields, so the **seam `Options<A>` stays**.

---

## 2. The swap (concrete)

### 2.1 `renderer_seam.rs` — delete the `DerivedConfig` placeholder

Remove the placeholder struct + impl (renderer_seam.rs:67-102) and re-point the
one internal use. Add a re-import so the seam `Options<A>.config` names the real
type:

```rust
// was: use rustty_renderer::size::Size; ...
use rustty_renderer::DerivedConfig;          // the real cross-platform config
```

`Options<A>.config` (renderer_seam.rs:190-194) re-types by that name — the field
comment updates from “the placeholder `DerivedConfig` above” to “the real
`rustty_renderer::DerivedConfig` (crate-root re-export); the seam `Options<A>`
still carries it because the real `rustty_renderer::Options` cannot hold the 3
apprt-typed fields below (apprt-api → renderer cycle)”:

```rust
pub struct Options<A: apprt::App + 'static> {
    pub config: DerivedConfig,               // now rustty_renderer::DerivedConfig
    pub font_grid: *mut SharedGrid,
    pub size: Size,
    pub surface_mailbox: SurfaceMailbox<A>,  // apprt-typed — real Options can't hold
    pub rt_surface: *mut A::Surface,
    pub thread: *mut Thread<A>,
}
```

The module doc (renderer_seam.rs:22-31, 71-77) updates: strike
“`renderer.Renderer.DerivedConfig` … Inhabited field-less struct with
`unimplemented!()` methods” from the placeholder list; add a one-line PORT NOTE
that `DerivedConfig` is now the real `rustty_renderer::DerivedConfig` (the
config plane is landed; only the windowing `Renderer`/xev `Thread` shapes remain
here). **G3b comment-coverage stays green** — the removed placeholder’s
reference-line citations (`generic.zig:536/574/651`) now live in the real
`generic.rs` `DerivedConfig`, so no reference line loses its Rust home.

### 2.2 `surface.rs` — `Surface::init` derivation

```rust
// surface.rs:1743
let renderer_config = match rendererpkg::DerivedConfig::init(alloc, config) {
    Ok(v) => v,
    Err(err) => { derived_config.deinit(); return Err(err.into()); }
};
```

`config` here is `&configpkg::Config` (= `&rustty_config::Config`), which impls
`RendererConfig` — so `C` infers to `Config` and `init<C>` runs the **real**
derivation. `alloc` is `Global` (matches `init`’s `alloc_gpa: Global`). The
`Options { config: renderer_config, ... }` construction at 1751-1763 is
unchanged (the seam `Options<A>` now carries a real `DerivedConfig`).

### 2.3 `surface.rs` — error wiring

`DerivedConfig::init` now returns `DerivedConfigError` (not the seam
`RendererError`). Add a dedicated `InitError` arm + `From` — the established
InitError doctrine (“typed as the union of the concrete sources”, surface.rs
InitError doc):

```rust
pub enum InitError {
    ...
    /// The renderer `DerivedConfig::init` — the config-link `oniRegex()` compile
    /// failures + arena OOM (rendererpkg::generic::DerivedConfigError).
    RendererDerivedConfig(rendererpkg::generic::DerivedConfigError),
    ...
}
impl From<rendererpkg::generic::DerivedConfigError> for InitError {
    fn from(err: rendererpkg::generic::DerivedConfigError) -> InitError {
        InitError::RendererDerivedConfig(err)
    }
}
```

so `return Err(err.into())` at 1748 resolves. The existing
`InitError::Renderer(RendererError)` arm + its `From` (surface.rs:1329/1355)
**stay** — still used by `Renderer::init`/`surfaceInit`/`finalizeSurfaceInit`/
`draw`. Extend the `Display`/`Debug` match for the new arm to keep the enum
exhaustive. (Alternative, if a distinct arm is judged too heavy: map at the
callsite `.map_err(|_| InitError::Renderer(RendererError))` — faithful to the
reference folding DerivedConfig.init’s error into the same inferred union, but
loses the source type. Recommend the dedicated arm.)

### 2.4 Everything else — unchanged, stays a seam

- `renderer_seam::Renderer` (field `renderer: Renderer`, surface.rs:254/258) +
  its 6 methods: **verbatim `unimplemented!()` bodies preserved** (R5). Real
  `Renderer::init` needs `Metal::init(NSView)`.
- `renderer_seam::Thread<A>` (`init`/`threadMain`/`deinit`) — verbatim seam
  (D6/xev).
- `renderer_seam::RendererError` — unchanged; still core’s InitError::Renderer
  payload + `draw()`’s return (surface.rs:2676).
- `Surface::draw` (surface.rs:2676) → `self.renderer.drawFrame(true)?` — still
  the windowing seam.

---

## 3. Boundary in `Surface::init` (the exact non-windowing / windowing line)

```
   NON-WINDOWING (real derivation, this slice)          │  WINDOWING (stays a seam)
─────────────────────────────────────────────────────┼──────────────────────────
rendererpkg::DerivedConfig::init(alloc, config)        │  Renderer::surfaceInit(rt_surface)
   → real derivation from real Config                   │     → unimplemented!() (surface.rs:1620,
   (a TYPE — compile-checked; exercised HEADLESS         │        EARLIER on the happy path)
    only via the T1/T2 unit tests, §4)                   │  Renderer::init(alloc, Options{ config:
                                                        │     <real>, rt_surface, surface_mailbox,
                                                        │     thread }) → unimplemented!() (Metal)
```

**The swap does NOT change `Surface::init`'s runtime reachability.** On the
`Surface::init` happy path, `Renderer::surfaceInit(rt_surface)` at surface.rs:1620
is an `unimplemented!()` that panics **before** control ever reaches the swapped
`DerivedConfig::init` at surface.rs:1743 (this mirrors the reference order —
Surface.zig:493 `Renderer.surfaceInit` runs before :552's renderer
`DerivedConfig.init`). `surfaceInit` stays a windowing seam, so `Surface::init`
still panics at that earlier point. The swap therefore does **not** "move the
panic one step later," and it does **not** remove a live `unimplemented!()` from
the `Surface::init` happy path.

The swap's actual value is twofold, both off the `Surface::init` runtime path:
(a) **compile-time typing** — `Surface::init`'s derivation is now typed against
the real `rustty_renderer::DerivedConfig` and its real `init<C>` derivation
(not the field-less placeholder); and (b) the **headless T1/T2 unit tests
(§4)**, which call `DerivedConfig::init` / `Message::initChangeConfig` DIRECTLY
(no `Surface`, no `surfaceInit`) and are the first place the real `impl
RendererConfig for Config` executes.

---

## 4. Core unit tests (core CAN use a real `Config` → real `DerivedConfig`)

Core deps `rustty-config`, so it constructs `Config::default_config()` and runs
the real `init<C = Config>` — the **first exercise anywhere** of `impl
RendererConfig for Config`. Add a `#[cfg(test)]` module in `surface.rs` (or a
`renderer_seam` test module):

**T1 — real DerivedConfig from real Config.**
```rust
#[test]
fn renderer_derived_config_from_real_config() {
    rustty_oniguruma::testing::ensure_init().unwrap();  // OnceLock C-init guard
    let cfg = rustty_config::Config::default_config();
    let d = rustty_renderer::DerivedConfig::init(
        allocator_api2::alloc::Global, &cfg).expect("init");
    // Assert extracted fields track Config defaults, proving the config-side
    // `impl RendererConfig for Config` accessors + init<C> clamps/casts:
    assert_eq!(d.font_thicken, cfg.font_thicken());          // bool passthrough
    assert!((0.0..=1.0).contains(&d.background_opacity));     // init clamps
    assert_eq!(d.faint_opacity, (cfg.faint_opacity()*255.0).ceil() as u8);
    // background/foreground = to_terminal_rgb of the config colors, etc.
    let mut d = d; d.deinit();  // frees links' onig handles (no Drop) — no leak
}
```

**T2 — real `Message::ChangeConfig` end-to-end (no `Surface` needed).**
```rust
#[test]
fn init_change_config_from_real_config() {
    rustty_oniguruma::testing::ensure_init().unwrap();  // OnceLock C-init guard
    let cfg = rustty_config::Config::default_config();
    let msg = rustty_renderer::Message::initChangeConfig(
        allocator_api2::alloc::Global, &cfg).expect("initChangeConfig");
    match msg {
        rustty_renderer::Message::ChangeConfig { .. } => {}  // both DerivedConfigs built
        _ => panic!("expected ChangeConfig"),
    }
    msg.deinit();  // frees impl.links onig handles then drops both boxes
}
```

Both are headless (no Metal, no NSView, no thread). They cover what the renderer
crate’s own `MockConfig` tests structurally **cannot**: the real
`Config`-accessor wiring. `Surface::init` itself is **not** unit-tested — it
still panics earlier at the `Renderer::surfaceInit` seam (windowing, §3).

**Oniguruma C-FFI note (parallel-test safety).** T1/T2 both reach
`link::Set::from_config` (generic.rs:480), which compiles the default config's
default link regex through the oniguruma C FFI. This is still headless /
non-windowing, but per memory `parallel-cargo-test-exposes-c-init-races` the
non-thread-safe global `onig_initialize` must be OnceLock-guarded so a parallel
`cargo test -p rustty-core` does not race it. The guard already exists —
`oni::testing::ensure_init()` (rustty-oniguruma `testing.rs`, `OnceLock`-backed,
exactly as the renderer's own `link.rs` tests use at link.rs:321/379/452). Core
already deps `rustty-oniguruma` (Cargo.toml:52), so T1/T2 must call
`rustty_oniguruma::testing::ensure_init().unwrap()` before the derivation.

---

## 5. Acyclicity (R4)

No new edges. Core already deps both `rustty-renderer` and `rustty-config`;
`DerivedConfig::init<C>` is generic (`C` bound by the renderer-defined
`RendererConfig` trait), so **no `renderer → config` edge** is introduced by
core binding `C = Config`. The `impl RendererConfig for Config` stays in
rustty-config (config → renderer, the existing G7 direction). Run
`.port/tools/gates/check_acyclic.sh` after the swap — expected: unchanged/green.

---

## 6. Dead-code + no-regression

- **Renderer crate:** untouched; its `DerivedConfig`/`Message` tests (MockConfig)
  stay green. The `pub use generic::DerivedConfig` is now reached by an external
  crate (core), so no new dead-code warnings.
- **Core:** the deleted seam `DerivedConfig` placeholder had no other referent;
  `RendererError`/`Renderer`/`Thread`/`Options` seams remain live (Surface
  fields + `init`/`draw`). New `InitError::RendererDerivedConfig` arm is
  constructed at 1743 (not dead). `#[cfg(test)]` tests reference the real types.
- **Behavior:** `Surface::init` runtime behavior is unchanged for any caller
  that reaches it (still panics at `Renderer::init`); the swap only makes the
  earlier derivation real. `cargo build --workspace` + `cargo test -p
  rustty-core` + `cargo test -p rustty-renderer` + `cargo test -p rustty-config`
  are the backstop.

---

## 7. Open risks

1. **Latent-until-windowing runtime value.** The swap does **not** make any part
   of `Surface::init` runnable: `Renderer::surfaceInit` at surface.rs:1620 is an
   earlier `unimplemented!()` on the happy path and panics **before** control
   reaches the swapped `DerivedConfig::init` at 1743 (§3). The swap does not
   move that panic and does not remove a live `unimplemented!()` from the
   `Surface::init` happy path — `surfaceInit` stays a windowing seam. The swap's
   value is entirely off the `Surface::init` runtime path: (a) compile-time
   typing of the real derivation, and (b) the headless T1/T2 unit tests, which
   are the *only* place the real `impl RendererConfig for Config` is ever
   exercised. Judged worth it on those two grounds.
2. **Error-path leak (latent).** On `Renderer::init` failure the reference frees
   the partially-built `DerivedConfig` via its own errdefer; in Rust the moved
   `renderer_config` is dropped *inside* `Renderer::init`, and `DerivedConfig`
   has **no `Drop`** (its `links` onig handles free only via explicit
   `deinit()`). Since `Renderer::init` is `unimplemented!()`, this path never
   runs today — but when `Renderer::init` lands it must call
   `options.config.deinit()` on its error path (reference errdefer parity). Flag
   for the Phase-3 R-d fill; note it in the `Renderer::init` seam comment.
3. **`InitError` surface growth.** Adding `RendererDerivedConfig` widens the
   public `InitError`; if downstream matches it exhaustively they need the new
   arm. Contained to core; acceptable (the enum is the documented “union of
   concrete sources”).
4. **G3b bookkeeping.** Removing the seam `DerivedConfig` placeholder must not
   drop reference-line coverage — its `generic.zig:536/574/651` citations must
   be confirmed present on the real `generic.rs` `DerivedConfig` (they are).
   Re-run the PORT-STATUS/G3b check after editing the renderer_seam module doc.
