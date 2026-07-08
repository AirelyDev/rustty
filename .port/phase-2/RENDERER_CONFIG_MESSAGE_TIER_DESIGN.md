# Renderer config-message / thread integration tier — design

Status: DESIGN (touches no `crates/`). Author milestone: finish the
`rustty-renderer` config-message / thread integration surface — the three seams
that were "config-coupled, deferred" and are now unblockable **because
`renderer.Renderer.DerivedConfig::init<C: RendererConfig>` is real** (the
`RendererConfig` accessor trait landed in `crates/rustty-renderer/src/config.rs`,
impl-ed for `Config` in `crates/rustty-config/src/renderer_config.rs`, acyclic).

Reference (read-only): `ghostty` Zig 0.15.2 —
`src/renderer/message.zig` (`Message.initChangeConfig` + `Message.deinit`) and
`src/renderer/Thread.zig` (`Thread.DerivedConfig` + its `init`).

Rules: PORT_RULES.md — R1 (100% fidelity), R4 (ACYCLIC — renderer gains **no**
`config` edge; `check_acyclic.sh` stays green), R5 (only what is *genuinely*
blocked stays an `unimplemented!()` seam). G3b comment-coverage SKIPS
`rustty-renderer`, so the adversary / verify pass is the ONLY R1 backstop.

---

## 0. Executive decision summary

| Seam | Verdict | Why |
|---|---|---|
| (1) `Message::initChangeConfig<C: RendererConfig>` | **LAND** | `DerivedConfig::init<C>` is real; the thread half (seam 3) is unblockable too, so the whole constructor is faithfully buildable. |
| (2) `Message::deinit` ChangeConfig arm | **LAND** | must call `r#impl.deinit()` (see §3 — no `Drop` on `link::Set`/`oni::Regex`), then the `Box`es drop. |
| (3) `thread::DerivedConfig::init<C: RendererConfig>` | **LAND (unblockable)** | `custom_shader_animation` is a dependency-free `enum(c_int)` — extractable through a new `RendererConfig::custom_shader_animation()` accessor. Recommended representation: **renderer-local mirror enum** (tradeoff vs. hoist in §4). |

Nothing in this tier stays a seam. All three land together (seam 1 *requires*
seam 3 — it constructs the `thread` box by calling `thread::DerivedConfig::init`).

Caller-bound analysis (§5): adding `<C: RendererConfig>` to both `init` fns
breaks **no** compile-caller. Acyclic proof (§7): no `renderer -> config` edge.

---

## 1. The coupling that unblocks this tier

`Message::initChangeConfig` (reference `message.zig`) builds a `ChangeConfig`
message by calling **both** DerivedConfig constructors:

```zig
pub fn initChangeConfig(alloc: Allocator, config: *const configpkg.Config) !Message {
    const thread_ptr = try alloc.create(renderer.Thread.DerivedConfig);
    errdefer alloc.destroy(thread_ptr);
    const config_ptr = try alloc.create(renderer.Renderer.DerivedConfig);
    errdefer alloc.destroy(config_ptr);

    thread_ptr.* = renderer.Thread.DerivedConfig.init(config);          // seam 3
    config_ptr.* = try renderer.Renderer.DerivedConfig.init(alloc, config); // real
    errdefer config_ptr.deinit();

    return .{ .change_config = .{ .alloc = alloc, .thread = thread_ptr, .impl = config_ptr } };
}
```

Because the constructor calls `Thread.DerivedConfig.init` **inline**, seam 1
cannot land faithfully (R1) unless seam 3 is also real — a `Box::new(thread::DerivedConfig {})`
that silently skips the `custom_shader_animation` extraction would be a
fidelity break. The two therefore land as one unit; seam 3's feasibility (§4)
is the gate for the whole tier.

The ported enum variant already has the right shape
(`crates/rustty-renderer/src/message.rs:95`):

```rust
ChangeConfig {
    thread: Box<thread::DerivedConfig>,
    r#impl: Box<DerivedConfig>,
}
```

The reference `alloc: Allocator` field collapses into `Box` ownership (the
documented precedent in the variant's PORT NOTE); `impl` → `r#impl`.

---

## 2. Seam 1 — `Message::initChangeConfig<C: RendererConfig>`

Replace the deferred placeholder body
(`crates/rustty-renderer/src/message.rs:177`, which is currently
`unimplemented!()` over a bare `<C>`) with the faithful construction. Keep the
verbatim reference body block above it (R1); the `#[allow(non_snake_case)]`
stays.

### Signature + body

```rust
/// Initialize a change_config message.
//
// PORT: reference `initChangeConfig(alloc, config: *const configpkg.Config)`.
// `C` is bounded by the renderer-defined `RendererConfig` accessor trait (R4/G7 —
// the renderer may not name `rustty_config::Config`); the concrete
// `C = rustty_config::Config` binds in core (`updateConfig`, surface.rs:4356).
// The two `alloc.create` + write pairs collapse into `Box::new`, exactly as the
// renderer/thread `DerivedConfig::init` calls fold their arena/heap allocs into
// owned Rust values. `errdefer config_ptr.deinit()` is subsumed: the two
// `DerivedConfig::init` calls are the only fallible/moving steps and the `?`
// early-return drops nothing that owns a C resource before `r#impl` is built
// (see the NOTE below on `DerivedConfig::init`'s own error path).
#[allow(non_snake_case)]
pub fn initChangeConfig<C: RendererConfig>(
    alloc: Global,
    config: &C,
) -> Result<Message, InitChangeConfigError> {
    // thread_ptr.* = renderer.Thread.DerivedConfig.init(config);
    let thread = Box::new(thread::DerivedConfig::init(config));

    // config_ptr.* = try renderer.Renderer.DerivedConfig.init(alloc, config);
    let r#impl = Box::new(
        DerivedConfig::init(alloc, config).map_err(|_| InitChangeConfigError)?,
    );

    Ok(Message::ChangeConfig { thread, r#impl })
}
```

Notes:

- `alloc: Global` — `allocator_api2::alloc::Global` (the crate's `Allocator`
  alias, already `use`d in message.rs). `DerivedConfig::init` takes `Global`
  and internally does nothing with it (its arena/CStrings are owned); it is
  threaded through for signature fidelity, matching `generic.rs`.
- `thread::DerivedConfig::init` is **infallible** (reference returns
  `DerivedConfig`, not `!DerivedConfig`) — no `?`, no `errdefer` analog.
- `DerivedConfig::init` returns `Result<_, DerivedConfigError>`; map into
  `InitChangeConfigError` (the existing opaque growth-point marker,
  message.rs:138). The reference's `errdefer alloc.destroy(thread_ptr)` on the
  *config-init* error path is automatic: if `DerivedConfig::init` returns `Err`,
  the already-built `thread` `Box` drops (freeing the heap slot); `thread`
  carries no C resource, so nothing leaks. Faithful to the reference errdefer
  ordering.
- The `use crate::config::RendererConfig;` import is added to message.rs (the
  bound needs the trait in scope). This adds **no** crate edge — `RendererConfig`
  is renderer-local.
- `InitChangeConfigError` stays a `struct`; `map_err(|_| InitChangeConfigError)`
  collapses `DerivedConfigError` into it (the two opaque markers are peers —
  message.rs:132's doc already says the inferred set includes
  `Renderer.DerivedConfig.init`'s set).

---

## 3. Seam 2 — `Message::deinit` ChangeConfig arm

**Critical correctness finding.** The current arm is empty
(`Message::ChangeConfig { .. } => {}`) because the variant was statically
unreachable. Once seam 1 lands it is reachable, and an empty arm would **leak an
onig regex handle per configured link**:

- `DerivedConfig` has **no** `Drop` — only a manual `deinit(&mut self)` that
  calls `self.links.deinit(Global)` (`generic.rs:536`).
- `link::Set` has **no** `Drop` — only manual `deinit(&mut self, alloc)` that
  calls `link.deinit()` per element (`link.rs:131`).
- `oni::Regex` has **no** `Drop` — only manual `deinit(&mut self)` freeing the
  `OnigRegex` C handle (`crates/rustty-oniguruma/src/regex.rs:48`; a workspace
  grep for `impl Drop for Regex` finds nothing).

So `Box`-drop alone frees the heap slot + runs the `ArenaAllocator` `Drop`, but
**does not** free the regex handles. The reference `v.impl.deinit()` is
load-bearing, and the port must reproduce it.

### Body

```rust
// PORT: reference deinit destroys the change_config heap allocs:
//   v.impl.deinit(); v.alloc.destroy(v.impl); v.alloc.destroy(v.thread);
// The `alloc.destroy`s are the two `Box` drops; `v.impl.deinit()` MUST run
// first (it frees the onig regex handles owned by `impl.links` — neither
// `DerivedConfig`, `link::Set`, nor `oni::Regex` impl `Drop`, so `Box`-drop
// alone would leak them). The `thread` box carries no C resource; it just drops.
#[allow(clippy::single_match)]
pub fn deinit(self: Message) {
    match self {
        Message::ChangeConfig { r#impl, thread: _ } => {
            // v.impl.deinit(); (then r#impl + thread boxes drop == alloc.destroy)
            let mut r#impl = r#impl;               // Box<DerivedConfig>
            r#impl.deinit();
        }
        _ => {}
    }
}
```

`let mut r#impl = r#impl;` rebinds the moved box as mutable so
`deinit(&mut self)` can be called (auto-deref `Box<DerivedConfig>` →
`&mut DerivedConfig`). At scope end, `r#impl` and the elided `thread` drop —
the two `alloc.destroy`s. Faithful ordering: deinit-then-free.

`Message::deinit` takes `self` by value (already the ported signature). Callers:
the reference `errdefer renderer_message.deinit()` in `updateConfig`. In the
ported `updateConfig` (surface.rs:4356) the message is `push`ed into the mailbox
(moved) on the success path; the errdefer's failure-path deinit is the caller's
concern and out of scope here. The mailbox drain (a later renderer-thread tier)
calls `Message::deinit` after processing — this arm makes that free correct.

---

## 4. Seam 3 — `thread::DerivedConfig::init<C>` (DECISION: unblockable → LAND)

Reference (`Thread.zig`):

```zig
pub const DerivedConfig = struct {
    custom_shader_animation: configpkg.CustomShaderAnimation,
    pub fn init(config: *const configpkg.Config) DerivedConfig {
        return .{ .custom_shader_animation = config.@"custom-shader-animation" };
    }
};
```

One field, one config read. `configpkg.CustomShaderAnimation` is
`enum(c_int) { false, true, always }` (Config.zig:5244) — a **dependency-free**
value enum (render-types deps only `rustty-util` + `rustty-datastruct`; it can
name this type with zero cycle risk). So the field **is** extractable through a
`RendererConfig`-style accessor. **Verdict: unblockable — LAND it.**

The only design choice is the *representation* of the accessor's return type,
since the renderer may not name `rustty_config::CustomShaderAnimation` (R4/G7).

### 4.1 Tradeoff — renderer-local mirror vs. hoist to render-types

**Option A — renderer-local mirror (RECOMMENDED).** Define
`CustomShaderAnimation` as a renderer-local enum in `thread.rs`, exactly like
`Color`/`TerminalColor` are renderer-local mirrors in `generic.rs`. The
config-side `impl` maps its own enum → the mirror with an exhaustive `match`
(the `map_color`/`map_terminal_color` free-fn precedent already in
`renderer_config.rs`).

- Blast radius: `rustty-renderer` (thread.rs: mirror enum + field + `init`
  body; config.rs: one accessor) + `rustty-config` (renderer_config.rs: one
  accessor impl + a `map_custom_shader_animation` free fn). **Zero** churn to
  config's value-type parse/format machinery.
- Drift risk (two definitions of an extern-C-ABI enum) is **neutralized** by the
  exhaustive `match` in the config-side map: adding a variant to the config enum
  fails to compile until the map (and mirror) are updated. A one-line
  `#[test]` round-trips all three variants.

**Option B — hoist to `rustty-render-types` (the 7th sibling).**
`CustomShaderAnimation` is definitionally the same category as the 6 DerivedConfig
value-enums already hoisted in Chunk 3a (`AlphaBlending`, `WindowColorspace`,
`FontShapingBreak`, `WindowPaddingColor`, `BackgroundImageFit`,
`BackgroundImagePosition`, `BackgroundBlur`). Single source of truth.

- Blast radius is **larger and orthogonal to this seam**: render-types gains the
  enum; config value_types.rs swaps its local `enum` for `pub use
  rustty_render_types::CustomShaderAnimation` **and** must convert its
  `bare_enum_names!(CustomShaderAnimation { … })` (value_types.rs:768 — which
  emits *inherent* `string_to_enum`/`to_name` methods) into **free functions**,
  because inherent impls on a foreign (render-types) type are orphan-rule
  forbidden. That is the exact conversion `AlphaBlending`/`WindowColorspace`
  underwent (their parse/format live as free fns at config_struct.rs:8666/8787;
  render-types lib.rs:63 codifies "PARSE/FORMAT stays a CONFIG concern"). Use-sites
  to update: `config_struct.rs:9906` (`CustomShaderAnimation::string_to_enum`),
  `c_get.rs:317` (`g_enum!`), `deep_equal.rs:64`, plus the `TypeId` site
  (737 — unaffected). It drags a config-value-type refactor into a milestone
  scoped to the message/thread seam, risking config's large test suite.

**Recommendation: Option A (mirror).** This milestone's mandate is the
message/thread seam, not config value-type consolidation. The mirror keeps the
change surgical, reuses the renderer-local-mirror precedent living in the very
same accessor surface, is fully acyclic, and the compiler-enforced exhaustive
map removes the only real downside (drift). Option B is the correct move *when
next touching config value-types* and is recorded as the future-consolidation
path (see §8 OPEN RISK O-1 for reviewer sign-off).

### 4.2 Concrete changes (Option A)

**`crates/rustty-renderer/src/thread.rs`** — replace the field-less placeholder
struct + `init<C>` seam. Keep the verbatim reference block (R1).

```rust
/// PORTS `configpkg.CustomShaderAnimation` (Config.zig:5244,
/// `enum(c_int){false,true,always}`). Renderer-LOCAL mirror (R4/G7 — the renderer
/// may not name `rustty_config::CustomShaderAnimation`; config already deps
/// renderer). `Config` maps its own enum into this via
/// `RendererConfig::custom_shader_animation` (config-side, acyclic — the
/// `Color`/`TerminalColor` mirror precedent). `#[repr(C)]` preserves the
/// extern-C-ABI ordering (`false`=0, `true`=1, `always`=2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum CustomShaderAnimation {
    False,
    True,
    Always,
}

/// Configuration we need derived from the main config.
//
// PORT: reference `Thread.DerivedConfig { custom_shader_animation }`. The
// field-less [ADV-FIX 2] placeholder is now the real single-field struct; core
// embeds it BY VALUE (renderer_seam.rs:315) and constructs it nowhere (Thread::init
// is a placeholder), so promoting it breaks no core site (verified: no
// `thread::DerivedConfig { … }` construction exists outside this crate).
pub struct DerivedConfig {
    pub custom_shader_animation: CustomShaderAnimation,
}

impl DerivedConfig {
    // pub fn init(config: *const configpkg.Config) DerivedConfig {
    //     return .{ .custom_shader_animation = config.@"custom-shader-animation" };
    // }
    //
    // PORT: `C` is bounded by the renderer-defined `RendererConfig` (R4/G7); the
    // concrete `C = rustty_config::Config` binds in core (via `initChangeConfig`).
    // Infallible (reference returns `DerivedConfig`, not `!DerivedConfig`).
    pub fn init<C: RendererConfig>(config: &C) -> DerivedConfig {
        DerivedConfig {
            custom_shader_animation: config.custom_shader_animation(),
        }
    }
}
```

Add `use crate::config::RendererConfig;` to thread.rs. Note the module doc's
"DEPENDENCY PLACEHOLDER — INHABITED" paragraph should be updated to reflect that
the field + real `init` have now landed (it currently says they're deferred).

**`crates/rustty-renderer/src/config.rs`** — add one accessor to the
`RendererConfig` trait:

```rust
// custom-shader-animation: config `CustomShaderAnimation` mapped to the
// renderer-local mirror (thread.rs). Feeds `Thread::DerivedConfig::init` — the
// draw/animation loop reads it to decide whether custom shaders animate.
fn custom_shader_animation(&self) -> crate::thread::CustomShaderAnimation;
```

**`crates/rustty-config/src/renderer_config.rs`** — impl the accessor + a map
free fn (the `map_color` precedent):

```rust
use rustty_renderer::thread::CustomShaderAnimation as RCustomShaderAnimation;
use crate::value_types::CustomShaderAnimation;

/// Map the config's own `CustomShaderAnimation` to the renderer-local mirror.
fn map_custom_shader_animation(a: CustomShaderAnimation) -> RCustomShaderAnimation {
    match a {
        CustomShaderAnimation::False => RCustomShaderAnimation::False,
        CustomShaderAnimation::True => RCustomShaderAnimation::True,
        CustomShaderAnimation::Always => RCustomShaderAnimation::Always,
    }
}

// … inside `impl RendererConfig for Config`:
fn custom_shader_animation(&self) -> RCustomShaderAnimation {
    map_custom_shader_animation(self.custom_shader_animation)
}
```

`self.custom_shader_animation` is the `Config` field (config_struct.rs:3846).

---

## 5. Compile-boundary / caller-bound analysis (R4)

Adding `<C: RendererConfig>` to `Message::initChangeConfig` and
`thread::DerivedConfig::init` (both currently bare `<C>`, both currently
`unimplemented!()`):

- **`Message::initChangeConfig` caller** — the only one is core
  `Surface::updateConfig` (surface.rs:4356):
  `rendererpkg::Message::initChangeConfig(self.alloc, config)?` where
  `config: &configpkg::Config` = `&rustty_config::Config`. `rustty_config::Config`
  **impls `RendererConfig`** (renderer_config.rs:54) and core deps both crates,
  so the impl is in scope → the bound is satisfied. `?` already flows through
  `impl From<InitChangeConfigError> for Error` (surface.rs:1253). **No break.**
- **`thread::DerivedConfig::init` caller** — the *real* one is only
  `Message::initChangeConfig` itself (same crate; the workspace grep finds no
  other call to `thread::DerivedConfig::init`). Core does **not** call it: core's
  config-swap path uses the core-local `renderer_seam::DerivedConfig::init` /
  `renderer_seam::Thread::init` placeholders (renderer_seam.rs:90/372), which are
  untouched. **No break.**
- **`thread::DerivedConfig` type embedders** — core embeds it by value
  (`renderer_seam.rs:315 pub config: thread::DerivedConfig`) but **constructs it
  nowhere** (`Thread::init` is `unimplemented!()`). Verified: no
  `thread::DerivedConfig { … }` / `DerivedConfig {}` literal anywhere outside
  `rustty-renderer`. Adding the `custom_shader_animation` field therefore breaks
  no construction site, and the type needs no new derive for core (renderer_seam
  `Thread<A>` derives nothing).
- **Acyclic (R4/G7):** every new name is renderer-nameable — `RendererConfig`
  (renderer-local), the renderer-local mirror `CustomShaderAnimation`,
  `Global`. The config-side accessor impl lives IN `rustty-config` (which already
  deps renderer — the `Message::ChangeConfig` back-edge). **No `renderer ->
  config` edge is created.** `check_acyclic.sh` was green pre-change and the
  change adds no edge (render-types Cargo deps stay `rustty-util` +
  `rustty-datastruct`; renderer Cargo gains nothing).

---

## 6. Tests (mock-`C`, in `rustty-renderer` — canNOT dep `rustty-config`, even dev)

The renderer cannot dep `rustty-config` (that IS the R4 cycle), so exercise the
seams through the **existing `#[cfg(test)] MockConfig`** in
`generic.rs::derived_config_tests` (impl `RendererConfig`, returns fixed values).
Add `fn custom_shader_animation(&self) -> CustomShaderAnimation` to `MockConfig`
returning `CustomShaderAnimation::True` (the config default `.true`).

The `initChangeConfig`/`deinit` tests live best in **message.rs** under a
`#[cfg(test)]` module with its own tiny `MockConfig` (or the `MockConfig` is
promoted to a shared `#[cfg(test)]` helper module `crate::test_support`; either
is fine — recommend a local mock in message.rs to keep the test self-contained).

### T1 — `init_change_config_builds_both_derived`

```rust
let msg = Message::initChangeConfig(Global, &MockConfig::default()).expect("init");
match msg {
    Message::ChangeConfig { thread, r#impl } => {
        // seam 3: the thread DerivedConfig carries the extracted enum.
        assert_eq!(thread.custom_shader_animation, CustomShaderAnimation::True);
        // seam 1: the renderer DerivedConfig has the extracted fields
        // (spot-check the non-trivial transforms already asserted by the
        // generic.rs oracle: faint_opacity @ceil(x*255), Color::to_terminal_rgb).
        assert_eq!(r#impl.faint_opacity, 255);
        assert_eq!(r#impl.foreground, RGB { r: 0xFF, g: 0xFF, b: 0xFF });
        // clean up (also exercises T2's path):
        Message::ChangeConfig { thread, r#impl }; // (reconstruct if consumed)
    }
    _ => panic!("expected ChangeConfig"),
}
```

(Match by value moves the boxes; assert through them before dropping. If the
same `msg` is needed for T2, build a fresh one there.)

### T2 — `change_config_deinit_frees_links` (the leak / free assertion)

The deterministic hazard is the onig-regex free. Give the mock a **non-empty
`links`** slice with one real `InputLink` whose `oni_regex()` succeeds, so
`DerivedConfig::init` allocates a live `OnigRegex` handle; then `Message::deinit`
must free it via `r#impl.deinit()`:

```rust
let msg = Message::initChangeConfig(Global, &MockConfigWithLink).expect("init");
// Sanity: the link made it into the impl DerivedConfig.
if let Message::ChangeConfig { ref r#impl, .. } = msg {
    assert_eq!(r#impl.links.links.len(), 1);
}
// The free: deinit runs r#impl.deinit() (frees the regex), then the boxes drop.
// A double-free / use-after-free here would abort; correct single-free returns.
msg.deinit();
```

Leak-detection notes (honest scope): a *silently leaked* `OnigRegex` is not
caught by plain `cargo test` (onig is C FFI; Miri can't run it, so the
`mirihar-*` harness precedent does not apply to this path). What T2 *does*
guarantee: the arm calls `deinit` exactly once and touches no freed memory (a
double-free aborts). True leak accounting is left to a CI leak sanitizer /
the adversary review (G3b skips renderer — this is the stated R1 backstop).
Optionally strengthen with a `#[cfg(test)]` allocation-counter wrapper only if
one already exists in the crate; do **not** add a bespoke global allocator for
this (over-engineering for a single-handle path).

### T3 — `derived_config_extracts_custom_shader_animation` (generic.rs oracle)

In `generic.rs::derived_config_tests::derived_config_init_extracts_fields`, add
after the existing asserts: assert the *renderer* `DerivedConfig` is unaffected
(it has no such field) — this test is really about the **thread** one, so put a
one-liner in a thread.rs `#[cfg(test)]` module:

```rust
let d = thread::DerivedConfig::init(&MockConfig::default());
assert_eq!(d.custom_shader_animation, CustomShaderAnimation::True);
```

### No-regression

The current renderer suite is **54 `#[test]`s** (`grep -rc '#[test]'`
crates/rustty-renderer/src ⇒ 54; the "52" in the brief is a slightly stale
count). All must stay green. The new field on `thread::DerivedConfig` and the
new trait method touch no existing assertion. Config-side: adding one accessor +
map fn to `renderer_config.rs` is additive; `cargo build -p rustty-config`
compile-verifies the impl (the trait method count must match, else it won't
build — the natural backstop the module doc already relies on).

---

## 7. Acyclic proof (R4 — the crux)

```
rustty-config  ──deps──▶  rustty-renderer  ──deps──▶  rustty-render-types
     │  (impl RendererConfig for Config,                    (mirror NOT hoisted
     │   map_custom_shader_animation —                       here — Option A)
     │   lives in the type's home crate)
     ▼
  no back-edge renderer ─▶ config is created
```

- New names in `rustty-renderer`: `RendererConfig` (local),
  `CustomShaderAnimation` (local mirror), `Global` (allocator_api2) — all
  renderer-nameable.
- The `Config → renderer` mapping lives in `rustty-config` (already deps
  renderer). No new edge.
- Run `bash .port/tools/gates/check_acyclic.sh` after landing — expected GREEN
  (it was GREEN pre-change; the change adds no Cargo edge).

---

## 8. Open risks

- **O-1 (representation choice — needs reviewer sign-off).** §4 recommends the
  renderer-local mirror (Option A) over hoisting (Option B). If the reviewer
  prefers single-source-of-truth now, execute Option B as a **precursor step**
  (hoist enum → render-types; convert `bare_enum_names!(CustomShaderAnimation)`
  to free fns; fix the 3 use-sites) landed separately, THEN the field/accessor
  on top — mirroring how Chunk 3a Part A hoisted its 6 types as its own commit.
  Either path satisfies R1/R4; the doc fully specifies A and scopes B.
- **O-2 (leak observability).** T2 cannot deterministically prove *no leak* of
  the onig handle in plain `cargo test` (C FFI, Miri-opaque). Mitigation: the
  double-free/UAF abort semantics of T2 + adversary review are the backstop
  (G3b skips renderer). Accepted; do not build a bespoke tracking allocator.
- **O-3 (module-doc drift).** thread.rs's module doc + the `DerivedConfig`
  "DEPENDENCY PLACEHOLDER — INHABITED" paragraph, and message.rs's PORT STATUS /
  the `ChangeConfig` variant's "not constructible via the public API" note, and
  the `deinit` arm's "statically unreachable" note, all currently assert these
  seams are deferred. They MUST be updated to "landed" when the code lands, or
  the comments contradict the code (an R1 review flag). Enumerated so the
  implementer doesn't miss them: message.rs lines ~5-11, ~92-94, ~141-151,
  ~188-197; thread.rs lines ~27-52.
- **O-4 (`errdefer` fidelity on the config-init error path).** §2's body relies
  on `thread` being built first and dropping cleanly if `DerivedConfig::init`
  errs. This matches the reference errdefer ordering only because `thread`
  owns no C resource. If `thread::DerivedConfig` ever gains a resource-owning
  field, revisit (add an explicit drop/deinit on the `?` path). Documented so
  the invariant is not silently violated later.

---

## 9. Landing checklist

1. thread.rs: mirror enum + `custom_shader_animation` field + real `init<C:
   RendererConfig>`; update module/placeholder docs (O-3).
2. config.rs: `RendererConfig::custom_shader_animation` accessor.
3. renderer_config.rs (config crate): impl accessor + `map_custom_shader_animation`.
4. message.rs: real `initChangeConfig<C: RendererConfig>` + real `deinit`
   ChangeConfig arm (`r#impl.deinit()`); `use crate::config::RendererConfig;`;
   update docs (O-3).
5. Tests T1/T2/T3 + `MockConfig::custom_shader_animation`.
6. `cargo test -p rustty-renderer` (54 → 57 green), `cargo build -p
   rustty-config`, `cargo build --workspace`, `bash
   .port/tools/gates/check_acyclic.sh` (GREEN).
```
