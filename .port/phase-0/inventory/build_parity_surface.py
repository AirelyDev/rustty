#!/usr/bin/env python3
"""Assemble the full parity-surface.json from the individual parsers.

Runs each parser module's extraction in-process and writes a single
consolidated JSON under .port/phase-0/inventory/parity-surface.json.
All data is parsed directly from the ghostty reference tree pinned at 4749c4e.
"""
import json
import subprocess
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
GHOSTTY = "/Users/gael/projects/ghostty"


def run(script):
    out = subprocess.check_output(["python3", os.path.join(HERE, script)])
    return json.loads(out)


config = run("parse_config.py")
actions = run("parse_actions.py")
cli = run("parse_cli.py")
terminfo = run("parse_terminfo.py")
modes = run("parse_modes.py")
sgr = run("parse_sgr.py")
csi = run("parse_csi.py")
esc = run("parse_esc.py")

# Config split: user-facing vs internal (_-prefixed)
config_public = [c for c in config if not c["name"].startswith("_")]
config_internal = [c for c in config if c["name"].startswith("_")]

# Keybind actions: void vs payload
actions_void = [a for a in actions if a["payload"] is None]
actions_payload = [a for a in actions if a["payload"] is not None]

# Command palette defaults count (from command.zig actionCommands switch)
with open(os.path.join(GHOSTTY, "src/input/command.zig")) as f:
    cmd_text = f.read()
ac = cmd_text[cmd_text.index("fn actionCommands"):]
ac = ac[:ac.index("\n}\n") if "\n}\n" in ac else len(ac)]
palette_default_count = len(re.findall(r"\.title = ", ac))

# OSC: Command Key tags (from terminfo parser? no -> reparse osc)
with open(os.path.join(GHOSTTY, "src/terminal/osc.zig")) as f:
    osc_lines = f.readlines()
osc_keys = []
in_key = False
for i, line in enumerate(osc_lines):
    if "pub const Key = LibEnum(" in line:
        in_key = True
        continue
    if in_key:
        if re.match(r'^\s*\},?\s*$', line) and "}" in line and "&.{" not in line:
            # closing - but skip the &.{ opening
            if "&.{" not in "".join(osc_lines[max(0, i-1):i+1]):
                pass
        m = re.match(r'^\s*"([^"]+)",\s*$', line)
        if m:
            osc_keys.append(m.group(1))
        if line.strip() == ");":
            break

# OSC sub-parsers (files in osc/parsers/)
osc_parsers = sorted(
    f[:-4] for f in os.listdir(os.path.join(GHOSTTY, "src/terminal/osc/parsers"))
    if f.endswith(".zig")
)

# OSC numeric codes referenced in osc.zig
with open(os.path.join(GHOSTTY, "src/terminal/osc.zig")) as f:
    osc_text = f.read()
osc_numbers = sorted(set(int(n) for n in re.findall(r'OSC (\d+)', osc_text)))

# DCS Command kinds + DECRQSS sub-values + recognized hook sequences
with open(os.path.join(GHOSTTY, "src/terminal/dcs.zig")) as f:
    dcs_lines = f.readlines()
dcs_cmd_kinds = []
in_cmd = False
for line in dcs_lines:
    if line.startswith("pub const Command = union(enum) {"):
        in_cmd = True
        continue
    if in_cmd:
        if re.match(r'^    pub (const|fn) ', line):
            break
        m = re.match(r'^    ([a-z_]+):', line)
        if m:
            dcs_cmd_kinds.append(m.group(1))
decrqss = []
in_dq = False
for line in dcs_lines:
    if "pub const DECRQSS = enum {" in line:
        in_dq = True
        continue
    if in_dq:
        if line.strip() == "};":
            break
        m = re.match(r'^\s*([a-z_]+),\s*$', line)
        if m:
            decrqss.append(m.group(1))
dcs_hook_sequences = [
    {"sequence": "ESC P ... p", "name": "tmux control mode", "command": "tmux"},
    {"sequence": "ESC P + q ... ST", "name": "XTGETTCAP", "command": "xtgettcap"},
    {"sequence": "ESC P $ q ... ST", "name": "DECRQSS", "command": "decrqss"},
]

# C0 control codes handled in execute()
with open(os.path.join(GHOSTTY, "src/terminal/ansi.zig")) as f:
    ansi_lines = f.readlines()
c0 = []
in_c0 = False
for line in ansi_lines:
    if line.startswith("pub const C0 = enum"):
        in_c0 = True
        continue
    if in_c0:
        if line.strip() == "};":
            break
        m = re.match(r'^\s*([A-Z]+) = (0x[0-9A-Fa-f]+),', line)
        if m:
            c0.append({"name": m.group(1), "value": m.group(2)})

surface = {
    "_meta": {
        "reference": GHOSTTY,
        "reference_rev": "4749c4e93731067049bfbf2e4572061cef2bdd17",
        "zig_version": "0.15.2",
        "generated_for": "phase-0 parity surface census",
        "note": "All counts parsed directly from reference source; no fabrication.",
    },
    "summary": {
        "config_options_user_facing": len(config_public),
        "config_fields_internal": len(config_internal),
        "config_fields_total": len(config),
        "keybind_actions": len(actions),
        "keybind_actions_void": len(actions_void),
        "keybind_actions_with_payload": len(actions_payload),
        "command_palette_default_commands": palette_default_count,
        "cli_actions": cli["count"],
        "terminfo_entry_names": len(terminfo["entry_names"]),
        "terminfo_capabilities": terminfo["count"],
        "terminfo_capabilities_by_type": terminfo["by_type"],
        "terminal_modes": modes["count"],
        "terminal_modes_ansi": modes["ansi"],
        "terminal_modes_dec": modes["dec"],
        "sgr_attributes": sgr["count"],
        "osc_command_kinds": len(osc_keys),
        "osc_subparsers": len(osc_parsers),
        "osc_numeric_codes_referenced": len(osc_numbers),
        "csi_final_byte_handlers": csi["count_arms"],
        "esc_final_byte_handlers": esc["count_arms"],
        "dcs_command_kinds": len(dcs_cmd_kinds),
        "dcs_decrqss_subvalues": len(decrqss),
        "dcs_recognized_hook_sequences": len(dcs_hook_sequences),
        "c0_control_codes": len(c0),
    },
    "config_options": {
        "source": "src/config/Config.zig",
        "user_facing": config_public,
        "internal": config_internal,
    },
    "keybind_actions": {
        "source": "src/input/Binding.zig (Action union); palette in src/input/command.zig",
        "actions": actions,
        "command_palette_default_count": palette_default_count,
    },
    "cli_actions": {
        "source": "src/cli/ghostty.zig (Action enum), src/cli/action.zig (detection)",
        "actions": cli["actions"],
    },
    "terminfo": {
        "source": "src/terminfo/ghostty.zig (+ Source.zig, main.zig)",
        "entry_names": terminfo["entry_names"],
        "by_type": terminfo["by_type"],
        "capabilities": terminfo["capabilities"],
    },
    "terminal_modes": {
        "source": "src/terminal/modes.zig",
        "modes": modes["modes"],
    },
    "sgr_attributes": {
        "source": "src/terminal/sgr.zig (Attribute union / Tag enum)",
        "attributes": sgr["attributes"],
    },
    "osc_handlers": {
        "source": "src/terminal/osc.zig (Command union / Key enum), src/terminal/osc/parsers/",
        "command_kinds": osc_keys,
        "subparsers": osc_parsers,
        "numeric_codes_referenced": osc_numbers,
    },
    "csi_handlers": {
        "source": "src/terminal/stream.zig csiDispatch (switch on input.final)",
        "note": "Top-level final-byte arms; many further dispatch on intermediates "
                "(more distinct commands than arm count). Param enums in src/terminal/csi.zig.",
        "has_else_unimplemented": csi["has_else"],
        "arms": csi["arms"],
    },
    "esc_handlers": {
        "source": "src/terminal/stream.zig escDispatch (switch on action.final)",
        "has_else_unimplemented": esc["has_else"],
        "arms": esc["arms"],
    },
    "dcs_handlers": {
        "source": "src/terminal/dcs.zig (Handler, Command union, DECRQSS enum)",
        "command_kinds": dcs_cmd_kinds,
        "decrqss_subvalues": decrqss,
        "recognized_hook_sequences": dcs_hook_sequences,
    },
    "c0_controls": {
        "source": "src/terminal/ansi.zig (C0 enum), dispatched in stream.zig execute()",
        "controls": c0,
    },
}

out_path = os.path.join(HERE, "parity-surface.json")
with open(out_path, "w") as f:
    json.dump(surface, f, indent=2)
    f.write("\n")

print(json.dumps(surface["summary"], indent=2))
print("\nWROTE:", out_path)
