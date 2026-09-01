# P6-W4 — core tier (rustty-renderer, rustty-core, rustty-config, rustty bin, rustty-apprt gtk, rustty-os)

Range `88f57ee66..20abdb50a`, END STATE ported from the oracle working tree at
`20abdb50a`. Branch `phase-6/w4-core-tier`.

## Coverage statement

Every oracle file the plan assigns to W4 was re-vetted against the range diff
and its rustty home located by grep (not assumed):

| oracle file | range delta | rustty home |
|---|---|---|
| `src/renderer/generic.zig` | +22 net (`97f57edcc`, `6688aa072`) | `crates/rustty-renderer/src/generic.rs` (macOS/Metal twin; `generic/linux_impl.rs` has no `display_link` — `DisplayLink == void` off macOS, so `syncDisplayLink` is the comptime early-return there) |
| `src/config/Config.zig` | +87 (`dfccdb2d4` + `e80ce2ed4`) | `crates/rustty-config/src/config_struct.rs`, `value_types.rs` |
| `src/Surface.zig` | +56/−? (`dfccdb2d4`) | `crates/rustty-core/src/surface.rs` |
| `src/main_ghostty.zig` | 1 line (`caf48a41e`) | `crates/rustty/src/main_rustty.rs` (N/A projection, see below) |
| `src/apprt/gtk/class/surface.zig` | `6cd684d5d`, `3e2c0fa2d`+`860cfb1d7` | `crates/rustty-apprt/src/gtk/class/surface.rs` |
| `src/apprt/gtk/class/surface_scrolled_window.zig` | `6cd684d5d` | `crates/rustty-apprt/src/gtk/class/surface_scrolled_window.rs` |
| `src/os/i18n_locales.zig` | +2 (`7aa476714`) | `crates/rustty-os/src/i18n_locales.rs` |

Downstream artefacts that DERIVE from the changed config enums and were
re-derived (never hand-edited): the three shell-completion goldens in
`crates/rustty-extra/testdata/{rustty.bash,_rustty,rustty.fish}` (see
"Derived goldens" below).

## Per-commit verdicts

| oracle SHA | subject | arrival verdict (evidence) | port |
|---|---|---|---|
| `97f57edcc` | renderer: vsync unfocused surfaces while dirty | **ABSENT** — `generic.rs:2712` (pre-port) still `self.visible && self.focused` | `sync_display_link` predicate → `visible && (cells_rebuilt \|\| animation_wake().is_some())` with the `!is_running()` start guard; reference comment block updated to end state |
| `6688aa072` | renderer: park DisplayLink while idle | **ABSENT** — no `sync_display_link` call at `update_frame`'s tail (`generic.rs:5298-5305`) nor in `draw_frame_raw`'s `!needs_redraw` arm (`:6186-6191`) | both calls added with the reference comments (`// Start the display link now that the rebuilt frame is ready.` / `// Resync the display link because we can probably pause ...`); the tail call runs before the deferred `font_shaper.endFrame()` exactly as the reference's `defer` ordering |
| `dfccdb2d4` | Implement needed modifications for issue #12600 | **ABSENT** — `value_types.rs:450` `CopyOnSelect { False, True, Clipboard }`, `:480` `MiddleClickAction { PrimaryPaste, Ignore }`; no `copy-on-select` row in `Config::compatibility()` (`config_struct.rs:9500-9580`); `surface.rs:5761` early-exit on `False`, `:8907` middle-click clipboard switch on `copy_on_select` | enums re-shaped to `{None, Primary, Clipboard, Both}` / `{PrimaryPaste, ClipboardPaste, Ignore}` (declaration order = oracle order, so `VALUE_NAMES`/completions/`to_name_z` follow); user-facing docs ported verbatim; default `.linux => .primary, else => .none`; `compat_copy_on_select` + table row (with the reference's PR-12604 comment); `setSelectionAndCopy` four arms, `mouseButtonCallback` `!= .none` and the two middle-click arms |
| `e80ce2ed4` | Fix build error | one change with `dfccdb2d4` (range-diff trap noted in the plan) — ported together as the END STATE | — |
| `caf48a41e` | main: fix inverted `allow_stack_tracing` condition | **N-A** — the whole `std_options.allow_stack_tracing` option was recorded N/A in `.port/phase-3/NON_PORTS.md` E36 (Zig-unwinder switch; Rust has no build-time `strip_debug_info` toggle and backtraces are `RUST_BACKTRACE`/panic-hook governed); `main_rustty.rs` carries `std_options` only as a comment projection (`:457-469`) which LACKED the option's block | reference block carried at its END STATE (`!builtin.strip_debug_info`) in the `std_options` projection comment + PORT NOTE; NON_PORTS E36 gains a phase-6 follow-up item. Verdict unchanged: nothing to run |
| `6cd684d5d` | gtk: fix stale pointers to property bindings | **PARTIAL** — the extra `binding.ref()` the fix adds is inherent in gtk-rs (`BindingBuilder::build()` returns a strong `glib::Binding` via `from_glib_none` over the (transfer none) `g_object_bind_property` result), so the pointer was never stale in the port; but the END STATE's `unbind()` calls were ABSENT in `Surface::dispose` (`surface.rs:702-714`, no `is_split_binding` handling at all), `SurfaceScrolledWindow::dispose` (`:108` bare `take()`) and the `set_surface` re-bind (`:221` bare `take()`, with a comment saying the reference does a bare unref — true at the pin, false now) | `unbind()` + drop (= `unref()` + null) in all three places; `bind_is_split` restructured to the reference's `const binding = ...; ref(); store` shape with the reference comment and a PORT NOTE explaining where the `ref()` lives |
| `3e2c0fa2d` + `860cfb1d7` | gtk: do not warn when gtk-xft-dpi is −1 | **ABSENT** — `surface.rs:1735` warns for every `dpi <= 0` | `if dpi != -1` guard with the reference comment + GTK docs link; `1.0` still returned for −1 |
| `7aa476714` | Add new translations to locales file | **ABSENT** — `i18n_locales.rs:50` ends at `"eu"`. Re-vetting the whole list against the end-state file also exposed a PRE-EXISTING gap: `"da"` (oracle `30e5eabe9`, before the pin — its `po/da.po` was ported in phase-4 `3d6fd16e` but the locales row was not) | `"da", "sr", "sr@latin"` appended in oracle order; the port's list is now token-for-token the oracle end state (34 entries, verified by extracting both lists). The `"da"` fix is outside the range but IS the file's end state; it is called out here rather than folded in silently |

### Range-diff traps honoured
- `dfccdb2d4` + `e80ce2ed4` ported as one change.
- Nothing in W4's files is reverted inside the range; all hunks are end state.

## Tests

### Reference tests added in the range for W4 files
None — `generic.zig`, `Config.zig`, `Surface.zig`, `main_ghostty.zig`, the two
gtk classes and `i18n_locales.zig` gain no `test {}` in `88f57ee66..20abdb50a`.

### Port-side regression tests (labelled `PORT-SIDE REGRESSION` in source)
| test | what it pins | negative control | run? |
|---|---|---|---|
| `rustty-renderer` `display_link_sync::sync_parks_the_link_while_idle_and_runs_it_while_dirty` | the end-state predicate: idle → parked, dirty → running (focused or not), second sync while running is a no-op, hidden never runs, visible+dirty runs again. Creates a REAL CoreVideo link; skips when `createWithActiveCGDisplays` fails | restoring `visible && focused` fails the first assertion (`left: Some(true), right: Some(false)`, "idle surface must park the link") | **RUN** — control executed on this host (link really created, not skipped): FAILED as predicted; restored and passes |
| `rustty-renderer` `display_link_sync::{sync_without_vsync_never_creates_a_link, sync_without_draw_now_never_creates_a_link, set_visible_tracks_visibility_state}` (gap-close regressions) | kept; doc of the third updated to the end-state predicate | (unchanged) | pass |
| `rustty-config` `compatibility_copy_on_select_true_false` | `true` → `primary` (linux/freebsd) / `clipboard` (else), `false` → `none`, the four new spellings parse bare, `bogus` still diagnoses+leaves the default, direct handler contract | removing the `copy-on-select` compatibility row (or the handler's `true` arm) → `InvalidValue` diagnostic, `cfg._diagnostics.empty()` fails | RUN (see log below) |
| `rustty-config` `middle_click_action_clipboard_paste` | `clipboard-paste` parses; `primary-paste` still the default | dropping the `ClipboardPaste` variant is a compile error; dropping its `bare_enum_names!` row → `InvalidValue` | RUN |
| `rustty-extra` `{bash,zsh,fish}::tests::completions_match_golden` | the regenerated goldens (byte-exact) | pre-port goldens FAILED against the new enums (observed before regeneration) — the derivation is machine-checked in both directions | RUN |

### Negative controls NOT run, and why
- `Surface.zig` arms (`setSelectionAndCopy`, `mouseButtonCallback` middle-click): `rustty-core` has no `Surface` unit harness (a `Surface` needs a live apprt surface + termio + renderer thread); the arms are a one-to-one transcription with the reference lines carried inline, checked by `cargo test -p rustty-core` compiling the exhaustive `match` (the removed `False`/`True` variants make any stale arm a compile error — that IS the control the type system runs).
- gtk `unbind()`/xft −1: the gtk module is `#[cfg(all(any(linux, freebsd), feature = "gtk"))]`; this macOS host has no gtk4/libadwaita pkg-config (`pkg-config --exists gtk4 libadwaita-1` → no), so the crate resolves to a no-op here. Ported with the same care, `rustfmt --check` clean; the coordinator's Linux lane compiles it (G-GTKFEAT). No reference test exists for either.
- `caf48a41e`: N/A — nothing executes.
- `7aa476714`: `staticLocale("sr")` would panic before / return after — trivially true; not worth a test the reference does not have.

## Derived goldens (re-derived, not hand-edited)
`crates/rustty-extra/testdata/{rustty.bash,_rustty,rustty.fish}` are "ghostty's
actually-generated file" fixtures; the oracle's generators
(`src/build/{bash,zsh,fish}_completions.zig`) are unchanged in the range and
walk the enum fields in declaration order, so the end-state output is
determined by the enum shapes. Regenerated from the port's generators
(`rustty_extra::{bash,zsh,fish}::completions()`) and diffed against the
committed files: the diff is EXACTLY two lines per file —
`false true clipboard` → `none primary clipboard both` and
`primary-paste ignore` → `primary-paste clipboard-paste ignore`. Nothing else
moved.

Left alone on purpose:
- `.port/phase-0/corpora/config/show-config.default{,-docs}.golden` — captured
  ORACLE bytes at the phase-0 pin (`PARITY_NAMING.md` "Stays ghostty"); no gate
  consumes them (grep), and they are not regenerated by the port.
- `.port/phase-0/inventory/parity-surface.json` — phase-0 oracle inventory; the
  `default` strings for `copy-on-select` are stale w.r.t. this range but
  `check_parity_surface.py` keys on `name` only (it never reads `default`).

## Consumer-shape changes W5 (Swift) / C-API must know about
- `rustty_config_get(cfg, "copy-on-select", &ptr)` still returns a C string
  (`g_enum!` → `to_name_z()`); the VALUE SET changed: `false|true|clipboard` →
  `none|primary|clipboard|both`. `middle-click-action` gains `clipboard-paste`.
  No struct/discriminant crosses the ABI (no `closed_enum_abi` entry for either;
  verified by grep of `crates/rustty-config/src/c_get.rs` — `g_enum`, not a
  numeric getter).
- No Swift consumer exists on either side: `grep -rn "copy-on-select|copyOnSelect|middle-click|middleClick"` over `macos/Sources` (rustty AND oracle) is empty. Nothing for W5.
- No `include/rustty-embed.h` change (the oracle's `ghostty.h` is untouched in the range).

## Gates
- G3b (`python3 .port/tools/gates/check_comment_coverage.py`), before → after this branch:
  - `G3b [config]: distinctive ref comments=2857 missing=6 review=14` → `missing=0 review=7`
  - `G3b [core]: distinctive ref comments=1244 missing=1 review=1` → `missing=0 review=0`
- `cargo clippy -p rustty-renderer -p rustty-config -p rustty-core -p rustty-os -p rustty-extra -p rustty --all-targets -- -D warnings`: clean; `cargo build --workspace`: clean.
- `cargo test --no-fail-fast -p rustty-renderer -p rustty-config -p rustty-core -p rustty-os -p rustty-extra`: renderer 92 passed / 1 ignored (+3 opengl_gate_parity), config 227 passed, core 34 (+1 +2), os 84 (+1), extra 34 (+1) — 0 failed.
- `rustfmt --check` on every touched file: clean.
- `.port/tools/gates` config-key gates: none exist by that name (`ls | grep -i conf` is empty); the config-shaped gates are G9 `check_parity_surface.py` (names only — unaffected) and the rustty-extra golden tests (re-derived above). No count was hand-edited anywhere.

## Open items
- GTK crate not compiled on this host (Linux-only lane) — coordinator's G-GTKFEAT lane.
- Pre-existing `"da"` locales-row gap fixed here (see the `7aa476714` row); the po catalogs for `sr`/`sr@latin` are W6's — until W6 lands, `staticLocale("sr")` succeeds while `po/sr.po` does not exist (harmless: the list only gates name canonicalisation; gettext falls back).
