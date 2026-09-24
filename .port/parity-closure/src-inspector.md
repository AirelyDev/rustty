# Parity closure checklist — src/inspector

4 open finding(s): 0 high, 1 medium, 3 low. Full evidence per row is in [`docs/parity-audit/src-inspector.md`](../../docs/parity-audit/src-inspector.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## Medium (1)

- [x] **S108-F1** · CONFIRMED · `INDIV` · missing-branch
  - **What:** modesTable omits report_visibility (?2033) and kitty_paste_events (?5522) rows
  - **Oracle:** `src/inspector/widgets/terminal.zig:591-616` → **Rustty:** `crates/rustty-inspector/src/widgets/terminal.rs:1220-1270, 1330`
  - **Audit's suggested fix:** Insert `(Mode::ReportVisibility, c"report_visibility")` between the ReportColorScheme and InBandSizeReports rows, and append `(Mode::KittyPasteEvents, c"kitty_paste_events")` after InBandSizeReports, so MODES matches the Mode enum's declaration order exactly (ideally derive the table from a single canonical list shared with modes.rs so future mode additions cannot drift).

## Low (3)

- [ ] **S109-F1** · DISPUTED · `INDIV` · missing-function
  - **What:** pub fn eventTable(events) has no rustty counterpart
  - **Oracle:** `src/inspector/widgets/termio.zig:435-473` → **Rustty:** `crates/rustty-inspector/src/widgets/termio.rs:303-310`
  - **Audit's suggested fix:** Add a `pub fn event_table(events: &VTEventRing)` mirroring termio.zig:435-473 (BeginTable "events" RowBg|Borders with early return, three TableSetupColumn calls, reverse-iterate rows with PushIDPtr/PopID, SameLine, %d seq, %s kind, %s raw_description).
- [ ] **S109-F2** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** dcs_hook/dcs_put/apc_* raw_description text differs from the oracle's Action.format output
  - **Oracle:** `src/inspector/widgets/termio.zig:587-594` → **Rustty:** `crates/rustty-inspector/src/widgets/termio.rs:1277-1304`
  - **Audit's suggested fix:** Hand-format the DCS payload to the depth-1 printValue shape (`<DCS type name>{ .intermediates = { ... }, .params = { ... }, .final = <decimal> }`) instead of `{:?}`, and document the chosen type-name prefix; add a KAT for a DcsHook description.
- [ ] **S109-F3** · DISPUTED · `INDIV` · behavioral-difference
  - **What:** Struct-typed metadata values (color_operation.requests, kitty_color_protocol.list) show core::any::type_name instead of Zig @typeName
  - **Oracle:** `src/inspector/widgets/termio.zig:748-751` → **Rustty:** `crates/rustty-inspector/src/widgets/termio.rs:1556-1564, 1417, 1428`
  - **Audit's suggested fix:** Emit fixed strings matching the Zig type names the oracle would print for these two fields (or document the exact divergence and pin it with a KAT).
