# macOS App — UI / Behavioral Parity Gaps (vs ../ghostty)

Scope: **runtime UI/behavior** parity of the shipping macOS app, distinct from
`MACOS_APP_GAPS.md` (which is exclusively about the blank-window render-attach,
now resolved). Trigger for this audit: *"View-menu items are missing their
keyboard shortcuts."*

Investigation date: 2026-07-15. Method: structural diff of both `macos/Sources`
trees + a cross-implementation C-ABI oracle (`config_trigger` run against both
prebuilt libraries over the exact macOS menu action list).

---

## 1. Headline: the Swift layer is at parity — the gap is a C-ABI bug

**Every Swift source file, the `MainMenu.xib`, `MenuShortcutManager`,
`Config.swift`, and the AppDelegate menu wiring are byte-identical to Ghostty
modulo the documented renames** (`Ghostty`↔`Rustty`, `ghostty`↔`rustty`,
`GHOSTTY_`↔`RUSTTY_`, bundle-id `com.mitchellh.ghostty`↔`io.airely.ghostty`).
124 Swift files checked; the 20 that "differ" differ *only* by those rename
classes. **There are no Swift-layer UI gaps.**

Menu key-equivalents are 100% populated at runtime: all `keyEquivalent`s in the
xib are empty and `AppDelegate.syncMenuShortcuts()` fills each one via
`config.keyboardShortcut(for:)` → `rustty_config_trigger(cfg, action, len)`.

The default keybind set is also at parity: `Keybinds::init()` has the exact same
74 `.put()` + 42 `.put_flags()` entries as Ghostty's `Config.zig` `Keybinds.init`.
Running rustty's `config_trigger_` over all 51 synced menu actions produces
**byte-identical** results to Ghostty (same OK/EMPTY status, tag, unicode
codepoint, and modifiers) — verified from the current source via `cargo test`.

So the *logic* is correct. The failure is at the **C-ABI boundary**.

---

## STATUS (2026-07-15): FIXED + gated

- **Fix applied.** `TriggerC.mods` (crates/rustty-input/src/binding.rs) now carries
  the packed `Mods::int()` widened to the header's `int`-wide slot instead of the
  raw 10-byte `Mods` struct → `size_of::<TriggerC>()` is 12, register-returned.
  `#[allow(improper_ctypes_definitions)]` removed from `rustty_config_trigger`.
  Compile-time `const _` freeze added (`== 12`). `cargo build --workspace` clean;
  all rustty-config/rustty-input tests pass.
- **Proven end-to-end.** After rebuilding RusttyKit.xcframework, the parity
  harness (§4) now prints `menu-shortcut parity OK — 51 actions resolve
  byte-identically`. The rustty oracle no longer crashes. (The macOS app must be
  rebuilt — `macos/build-rustty.sh` — to pick up the new library.)
- **Class gated broadly.** New `.port/tools/gates/check_embed_h_abi.sh` (CI id
  **A4**) clang-probes every umbrella-header aggregate and HARD-fails if any
  by-value-return type (the sret crash class) lacks a matching Rust `@embed-abi`
  size-freeze — the exact coverage gap that let TriggerC through. The previously
  by-value-return-uncovered `Diagnostic` (8) and `Info` (24, its abi_layout.rs
  "DEFERRED" note was stale) are now frozen too; both were already correct.
  Result of the class sweep: **TriggerC was the only actual mismatch** among the
  8 by-value-return functions / 6 types.

---

## 2. ROOT CAUSE — `rustty_config_trigger` struct-return ABI mismatch  (CONFIRMED, FIXED)

`rustty_config_trigger` returns `TriggerC` **by value**. The generated embedding
header (`include/rustty.h`, `RusttyKit.xcframework/.../rustty.h`) declares:

```c
typedef struct { tag(4); key(union,4); rustty_input_mods_e mods(4); } rustty_input_trigger_s;  // 12 bytes
```

But the Rust `TriggerC` is:

```rust
#[repr(C)] struct TriggerC { tag: TriggerCTag, key: TriggerCKey, mods: Mods }   // size_of == 20
```

because `TriggerC.mods` embeds the full **non-`repr(C)`** `Mods` struct
(`crates/rustty-vt/src/input/key_mods.rs:82` — 6 `bool`s + a `Side` = 10 bytes),
instead of the packed `u16` the reference uses (`key.Mods = packed struct(u16)`)
and that the header projects as the 4-byte `rustty_input_mods_e` enum.

| | Rust impl | Generated header / every C+Swift caller |
|---|---|---|
| `sizeof(rustty_input_trigger_s)` | **20** | **12** |
| AAPCS64 return convention | **> 16 → indirect (x8 sret ptr)** | **≤ 16 → registers x0/x1** |

The caller expects a register return and never sets up `x8`, so Rust's sret write
lands on a garbage pointer → **deterministic `EXC_BAD_ACCESS` / SIGBUS** the
instant `config_trigger` is invoked across the C boundary. Reproduced by the
oracle harness (§4): the rustty oracle crashes on the first call, right after the
`default shell` config log; the ghostty oracle (true 12-byte ABI) runs clean.

The `#[allow(improper_ctypes_definitions)]` on the fn
(`crates/rustty-config/src/c_api.rs:177`) is silencing exactly the lint that
flags this. The PORT NOTE there explicitly defers "C-layout unification" of
`Mods` to rustty-input — it was a known, un-closed deferral.

**Fix location:** `crates/rustty-input/src/binding.rs:3264` (`Trigger::cval`) sets
`mods: self.mods`. Either (a) change `TriggerC.mods` to a packed `u16`
(`self.mods.int()`) so `sizeof == 12` and matches the reference's `packed(u16)`
exactly, or (b) make `Mods` itself an FFI-safe `#[repr(transparent)]`/`repr(C)`
2-byte type everywhere (the "unification in rustty-input" the note anticipated).
Approach (a) is the minimal faithful fix; (b) closes the whole `improper_ctypes`
class in one move but touches every `Mods` user.

**Blast radius:** any embedding C-ABI fn that returns/takes a `Mods`-bearing
struct *by value* shares this defect. `config_trigger` is the only by-value
*return* found; the other `#[allow(improper_ctypes_definitions)]` sites
(`rustty_app_new` etc.) pass `Options`/`ActionC` **by pointer**, which is
ABI-safe for the pointer itself — but any callback that passes `ActionC`/`Mods`
by value into a Swift function pointer needs the same audit once (a) or (b) lands.

**Regression gate to add with the fix:**
`const _: () = assert!(std::mem::size_of::<TriggerC>() == 12);` in rustty-input,
plus the harness below in CI.

> Runtime-manifestation caveat: the crash is proven in a standalone C harness
> that links the shipped `libRusttyKit.a`. Whether the shipping app crashes on
> launch or degrades to empty shortcuts depends on how Swift's importer lowers
> the (mis-sized) return — but the ABI is objectively wrong and must be fixed
> regardless; it is the direct explanation for absent/incorrect menu shortcuts.

---

## 3. Parity tooling gap (what `.port/` was missing)

`.port/tools/check-vt-struct-layouts.mjs` is a real ABI-size gate, but it only
covers the **`include/rustty/vt.h`** family (via `rustty_type_json()` from the
vt-c wasm). **Nothing gated the macOS *embedding* header (`rustty.h` /
RusttyKit).** That is precisely why a 20-vs-12 struct-return mismatch shipped
undetected. New artifacts added by this audit:

- `.port/tools/macos-parity/menu-shortcut-parity.sh` — builds a C oracle against
  BOTH prebuilt libs, resolves the full macOS menu action list through each
  `config_trigger`, diffs status/tag/key/mods. Fails on any divergence **or** a
  rustty crash. (Currently FAILS on the §2 crash — by design.)
- `.port/tools/macos-parity/{rustty,ghostty}_trigger_oracle.c` — the oracle TUs.
- **`.port/tools/gates/check_embed_h_abi.sh` (BUILT, CI id A4)** — the embedding-
  header ABI-size gate. Clang-probes every umbrella aggregate; hard-gates the
  by-value-return crash class against Rust `@embed-abi` size-freezes; prints a
  full 57-aggregate coverage ledger (16/57 frozen — every by-value return plus
  the pointer-passed structs abi_layout.rs already pinned). The remaining 41 are
  pointer-passed (no sret hazard); freezing them is a follow-up ledger item, not
  a crash risk.

Note: `chostty_ipc_target_s` in the ledger is an UPSTREAM ghostty.h typo,
preserved verbatim by the rename (per check_rustty_h_abi.sh's typo-preservation
policy) — not a rustty defect.

---

## 4. Reproduction

```
# from repo root, with ../ghostty built (GhosttyKit.xcframework present)
.port/tools/macos-parity/menu-shortcut-parity.sh
#  -> ghostty oracle: 51 actions resolve clean
#  -> rustty oracle : SIGBUS in rustty_config_trigger (the §2 bug)
```

Once §2 is fixed, the harness prints `menu-shortcut parity OK — 51 actions
resolve byte-identically`.

---

## 5. Other gaps scanned — none found in this layer

- Swift sources: parity (see §1).
- `MainMenu.xib` + all feature `.xib`s: parity.
- Default keybinds (`Keybinds::init`): parity (74 put + 42 put_flags).
- `config_trigger` *logic* (current source, Rust-side): parity for all 51 menu
  actions (status/tag/codepoint/mods identical to ghostty).

The single actionable finding is §2.
