# A3 re-pin — `oracle-ghostty-h.sha256` (P4-W5 Bundle E, 2026-08-18)

The A3 gate (`check_rustty_h_abi.sh`) freezes the oracle `include/ghostty.h`
by SHA-256 (`.port/tools/oracle-ghostty-h.sha256`). The Phase-4 oracle-pin
advance `6ad1fe7d8 -> 12967b68f` changed `ghostty.h` (9 in-range commits,
net +32/−6 lines), so A3 was deliberately left BLOCKED-red until the header
port landed (assessment C §6 gap #2: "the red is currently telling the
truth"). This bundle lands the header end state in `include/rustty-embed.h`
and re-pins in the SAME patch.

    old pin: 36ca1c10cd07094abbf77cb14c2531899ca74c089a62f6f6cdeb07aa4927b2af  (ghostty.h @ 6ad1fe7d8)
    new pin: aa01353ddc1a04470efe5e4cb642350db4d1ebc25054a7784c610165b8ea8a2b  (ghostty.h @ 12967b68f)

## The absorbed `ghostty.h` delta (all 8 surviving hunks; the
## `ghostty_surface_is_at_prompt` pair `bdb566068`/`c80c37362` cancels)

| upstream | hunk | rustty-embed.h |
|---|---|---|
| `d6248a32d` | header doc rewritten: "internal embedder API, a.k.a. libghostty-internal" | doc paragraph mirrored (token-renamed) |
| `ad96613a8` | `ghostty_action_export_terminal_io_s` struct + `GHOSTTY_ACTION_EXPORT_TERMINAL_IO` tag (MID-ENUM insert after `RENDER_INSPECTOR`) + union arm | `rustty_action_export_terminal_io_s` + tag 31 + union arm |
| `8b7c57c75` | `GHOSTTY_PROMPT_TITLE_WINDOW` + `GHOSTTY_ACTION_SET_WINDOW_TITLE` (MID-ENUM insert after `SET_TAB_TITLE`) | both mirrored (tag 35) |
| `25b2d8a38` | `GHOSTTY_ACTION_MOVE_TAB_TO_NEW_WINDOW` (appended tail) | tag 68 |
| `0a183c923` | `ghostty_action_open_config_e` enum + `open_config` union arm | mirrored |
| `77537c806` | `GHOSTTY_ACTION_OPEN_URL_KIND_OSC8` (appended) | mirrored |
| `034506f14` | `GHOSTTY_IPC_ACTION_NEW_TAB` (MID-ENUM insert after `NEW_WINDOW`) | mirrored |

No `GHOSTTY_API` function was added or removed in the range (the
`is_at_prompt` add was reverted in-range), so `CABI_SYMBOLS.tsv` needed no
new rows and A3's name-mapped function-set equivalence is unchanged.

**ABI consequence (embed-abi-byvalue-sret-gate memory):** the two mid-enum
`ghostty_action_tag_e` inserts renumber every later tag — the positional map
`action_key_positional_ordinals` (crates/rustty-apprt-c/tests/abi_layout.rs)
was regenerated from the header in the same patch: cardinality 66 -> 69,
tail `COPY_TITLE_TO_CLIPBOARD=65` -> `MOVE_TAB_TO_NEW_WINDOW=68`.
`rustty_action_u` stays 24 bytes (`ActionC` 32) — asserted at both freeze
sites; `rustty_ipc_action_u` unchanged (16).

Verified in this patch: token-renamed oracle `ghostty.h` vs
`include/rustty-embed.h` differ ONLY in the standing 4-line rustty preamble
(+ one trailing-whitespace line upstream carries).
