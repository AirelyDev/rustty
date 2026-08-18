#!/usr/bin/env python3
"""Diff rustty_type_json (renamed) against the oracle ghostty_type_json dump.

Expected deviations (the P4-W5 bundle sequencing deferrals) are listed
explicitly; anything else is a FAILURE.
"""
import ctypes, json, sys

RUSTTY_DYLIB = sys.argv[1]
ORACLE_JSON = sys.argv[2]

lib = ctypes.CDLL(RUSTTY_DYLIB)
f = lib.rustty_type_json
f.argtypes = ()
f.restype = ctypes.c_char_p
raw = f()
mine = json.loads(raw)

oracle = json.loads(open(ORACLE_JSON, "rb").read())

# R2 rename: mine -> oracle namespace.
def rename(o):
    if isinstance(o, dict):
        return {rename_key(k): rename(v) for k, v in o.items()}
    if isinstance(o, list):
        return [rename(v) for v in o]
    if isinstance(o, str):
        return o.replace("Rustty", "Ghostty").replace("RUSTTY_", "GHOSTTY_")
    return o

def rename_key(k):
    return k.replace("Rustty", "Ghostty")

mine = rename(mine)

DEFERRED_TYPES = {
    # Bundle R
    "GhosttyCellsView", "GhosttyRenderStateCursor",
    # Bundle M
    "GhosttyTerminalModeConfig", "GhosttyTerminalUnknownSequence",
    "GhosttyTerminalUnknownStringSequence", "GhosttyTerminalUnknownSequenceValue",
    "GhosttyTerminalUnknownSequenceTag",
    # Bundle S
    "GhosttyReader", "GhosttyWriter", "GhosttySnapshotDecoderData",
    "GhosttySnapshotDecoderOption", "GhosttySnapshotDecoder",
}
DEFERRED_ENUM_VALUES = {
    "GhosttyResult": {"IO_ERROR", "LIMIT_EXCEEDED"},
    "GhosttyRenderStateData": {"CURSOR", "COLORS"},
    "GhosttyRenderStateRowData": {"CELLS_RAW"},
    "GhosttyTerminalOption": {"CONTINUATION_MAX_BYTES", "TITLE_REPORT",
        "MODE_DEFAULT", "MODE", "UNKNOWN_SEQUENCE", "UNKNOWN_MAX_BYTES",
        "TERMINFO_NAME"},
    "GhosttyTerminalData": {"CONTINUATION_MAX_BYTES", "MODE", "VT_GROUND",
        "CURSOR_AT_PROMPT"},
}

fails = []
notes = []

# Envelope.
for key in ("schema", "abi", "dirty"):
    if mine.get(key) != oracle.get(key):
        fails.append(f"envelope {key}: mine={mine.get(key)!r} oracle={oracle.get(key)!r}")
for key in ("library_version", "commit"):
    if mine.get(key) != oracle.get(key):
        notes.append(f"identity {key}: mine={mine.get(key)!r} oracle={oracle.get(key)!r}")

mt, ot = mine["types"], oracle["types"]

for name in ot:
    if name not in mt:
        if name in DEFERRED_TYPES:
            notes.append(f"deferred type absent (expected): {name}")
        else:
            fails.append(f"MISSING type: {name}")
for name in mt:
    if name not in ot:
        fails.append(f"EXTRA type: {name}")

# Order check: mine's order must equal oracle's order with deferred rows removed.
oracle_order = [n for n in ot if n not in DEFERRED_TYPES]
mine_order = list(mt)
if oracle_order != mine_order:
    fails.append("type declaration ORDER differs")
    for i, (a, b) in enumerate(zip(oracle_order, mine_order)):
        if a != b:
            fails.append(f"  first order divergence at index {i}: oracle={a} mine={b}")
            break

def diff_value(path, a, b):
    if type(a) is not type(b):
        fails.append(f"{path}: type {type(a).__name__} vs {type(b).__name__} ({a!r} vs {b!r})")
        return
    if isinstance(a, dict):
        for k in a:
            if k not in b:
                fails.append(f"{path}/{k}: missing in mine")
            else:
                diff_value(f"{path}/{k}", a[k], b[k])
        for k in b:
            if k not in a:
                fails.append(f"{path}/{k}: extra in mine")
        # key order
        if list(a) != list(b) and all(k in b for k in a) and all(k in a for k in b):
            fails.append(f"{path}: key ORDER differs: {list(a)} vs {list(b)}")
    elif a != b:
        fails.append(f"{path}: {a!r} vs {b!r}")

for name in ot:
    if name not in mt:
        continue
    o_t = ot[name]
    m_t = mt[name]
    if o_t.get("kind") == "enum":
        o_vals = dict(o_t["values"])
        deferred = DEFERRED_ENUM_VALUES.get(name, set())
        for d in deferred:
            if d in o_vals and d not in m_t.get("values", {}):
                notes.append(f"deferred enum value absent (expected): {name}.{d}")
                del o_vals[d]
        o_t = dict(o_t, values=o_vals)
        # order for enums: compare orders with deferred dropped
        o_order = [k for k in o_t["values"]]
        m_order = list(m_t["values"])
        if o_order != m_order:
            fails.append(f"{name}: enum value ORDER differs")
        for k in o_vals:
            if k not in m_t["values"]:
                fails.append(f"{name}.{k}: missing enum value")
            elif o_vals[k] != m_t["values"][k]:
                fails.append(f"{name}.{k}: {o_vals[k]} vs {m_t['values'][k]}")
        for k in m_t["values"]:
            if k not in o_vals:
                fails.append(f"{name}.{k}: extra enum value")
        for meta in ("kind", "size", "align", "underlying", "prefix"):
            if o_t.get(meta) != m_t.get(meta):
                fails.append(f"{name}.{meta}: {o_t.get(meta)!r} vs {m_t.get(meta)!r}")
    else:
        diff_value(name, o_t, m_t)

print(f"types: oracle={len(ot)} mine={len(mt)}")
for n in notes:
    print("NOTE:", n)
if fails:
    print(f"\n{len(fails)} FAILURES:")
    for x in fails[:80]:
        print("FAIL:", x)
    sys.exit(1)
print("\nOK: manifest matches the oracle modulo the declared deferrals.")
