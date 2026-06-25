//! rustty-vt input
//!
//! PORTS: ghostty src/input/{key,key_encode,mouse,mouse_encode,paste,function_keys,kitty,config}.zig (VT input slice)
//!
//! PORT STATUS: stub

pub mod config;
pub mod function_keys;
pub mod key;
pub mod key_encode;
pub mod key_mods;
pub mod kitty;
pub mod mouse;
pub mod mouse_encode;
pub mod paste;
