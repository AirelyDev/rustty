//! PORTS: ghostty src/input/Binding.zig — the `test{}` blocks, split by topic
//! under `tests/`; the shared test helpers stay here.

use super::*;
use rustty_cfg_fmt::{entry_formatter, AllocatingWriter};

// PORT NOTE: convenience constructors mirroring the Zig struct literals used
// pervasively in the tests (`.{ .key = .{ .unicode = 'a' } }` etc.).
fn uni(c: u32) -> TriggerKey {
    TriggerKey::Unicode(c)
}

// PORT NOTE: test helpers for the `Set` value accessors. The reference reads
// `entry.value_ptr.*.leaf` / `.leaf_chained` directly; these mirror that.
fn as_leaf(v: &Value) -> &Leaf {
    match v {
        Value::Leaf(l) => l,
        _ => panic!("expected leaf"),
    }
}
fn as_leaf_chained(v: &Value) -> &LeafChained {
    match v {
        Value::LeafChained(l) => l,
        _ => panic!("expected leaf_chained"),
    }
}
// `try s.chain_parent.?.key_ptr.format(&buf.writer); expectEqualStrings(...)`
fn chain_parent_key_str(s: &Set) -> Vec<u8> {
    let mut buf = AllocatingWriter::init();
    s.chain_parent().unwrap().key.format(&mut buf).unwrap();
    buf.written().to_vec()
}
fn chain_parent_value(s: &Set) -> &Value {
    let cp = s.chain_parent().unwrap();
    let set = resolve_set_ref(s, &cp.set_path);
    set.get_value(cp.key).unwrap()
}
fn resolve_set_ref<'a>(s: &'a Set, path: &[Trigger]) -> &'a Set {
    let mut cur = s;
    for t in path {
        match cur.get_value(*t).unwrap() {
            Value::Leader(child) => cur = child.as_ref(),
            _ => unreachable!(),
        }
    }
    cur
}

mod action;
mod chain_format;
mod parser;
mod set;
mod trigger;
