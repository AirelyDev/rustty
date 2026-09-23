# Where Rustty deliberately differs from Ghostty

Rustty is a full-fidelity port of Ghostty: by default it behaves exactly as Ghostty does. The
exceptions are Ghostty bugs. Where Ghostty crashes, has undefined behaviour, leaks, deadlocks, or
visibly fails to do what its own code clearly intends (for example, output silently lost because
a buffer is never flushed), Rustty does what Ghostty meant to do instead, and the difference is
listed here.

Each entry names the Ghostty behaviour, what Rustty does, and the record in
`.port/parity-closure/UPSTREAM-DEFECTS.md` (the `UD` id), which has the reference location, the
measurements, and a proposed upstream fix. The rule behind this is R13 in `.port/PORT_RULES.md`.

Entries are added by the task that makes the change. A backfill of every earlier R13 fix is in
progress; until it lands, `UPSTREAM-DEFECTS.md` is the complete list.

| Area | Ghostty | Rustty | Record |
|---|---|---|---|
