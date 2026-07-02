//! PARITY (DEFERRED SEAM, R4 / D6): the `xev` event-loop backend surface used
//! by termio.
//!
//! Every termio reference file imports `const xev =
//! @import("../global.zig").xev` — the libxev async event-loop backend
//! (`@import("xev").Dynamic`). The handle TYPES (`Async`/`Loop`/`Completion`/
//! `Timer`/... + error sets) were declared here as termio-local placeholders
//! through integration-tier stage 4; core chunk C0 HOISTED them verbatim into
//! the shared `rustty-xev` crate (CORE_BLUEPRINT §5 "xev handle unification":
//! core hands `render_thread.wakeup`, an `xev.Async`, to
//! `termio::Options.renderer_wakeup`, so the placeholder types must be one
//! crate both consumers name). `rustty-xev` is also the crate the D6 decision
//! already named as libxev's future home — filled in place when the real port
//! lands. This module remains so every in-crate `use crate::xev_seam as xev`
//! path (the faithful map of the reference's per-file `const xev = ...`
//! import) is unchanged. See .port/phase-2/DEFERRALS.md ("rustty-termio
//! stage 2").

pub use rustty_xev::*;
