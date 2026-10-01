//! Tests for `value_types`, split by topic: `limit_colors`, `repeatable`,
//! `window_misc` (also `WorkingDirectory`, `FontStyle`, `AlphaBlending`,
//! `BackgroundBlur`, `WindowDecoration`), `command` (`RepeatableCommand`) and
//! `keybinds`.

use super::*;
use rustty_cfg_fmt::{entry_formatter, AllocatingWriter};

mod command;
mod keybinds;
mod limit_colors;
mod repeatable;
mod window_misc;
