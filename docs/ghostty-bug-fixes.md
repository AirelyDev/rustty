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
| Crash reporting (macOS) | The Sentry database goes to `~/Library/Caches` when `XDG_CACHE_HOME` is set, and to the XDG cache when it is not: the reverse of what its own comment says. | Uses the XDG cache when `XDG_CACHE_HOME` is set and non-empty, and `~/Library/Caches` otherwise, as the comment says. | UD21 |
| Crash reporting | Writing out a crash-report envelope whose headers are nested more than 256 levels deep crashes (the JSON writer asserts its depth limit instead of returning an error). | Returns an error instead of crashing, and still reads such an envelope. | UD182 |
| Kitty graphics | A file-medium image sent without `s=`, or a zlib payload, of exactly the 400 MiB limit is rejected, although every other size check accepts exactly the limit (the same file loads when sent with `s=`). | Accepts exactly the limit and rejects anything larger, on every path. | UD140 |
| Terminal protocols | Numbers in the glyph protocol, kitty graphics, OSC 4/104 color indices, OSC 66 text sizing, the OSC 133 exit code and OSC 9 accept Zig's `_` digit separator (`1_0` is read as 10), although Ghostty's own OSC 99 parser refuses it. | Refuses `_` in those numbers, as Ghostty's OSC 99 parser does. | UD141 |
| Version string | A development build's version reads `X.Y.Z-<branch>-+<hash>`: the branch name is sanitized before git's trailing newline is trimmed, so the newline becomes a `-` and the trim meant to remove it does nothing (a regression from upstream `ad9225a4c`). It shows in `+version`, XTVERSION, `TERM_PROGRAM_VERSION` and dist tarball names. | Trims the newline first, then sanitizes, so the version reads `X.Y.Z-<branch>+<hash>`, as it did before that commit. | UD150 |
| `+list-colors` | With `--plain`, or when its output is piped or redirected, the end of the colour list is missing: the output buffer is never flushed. | Prints the whole list. | UD22 |
| `+list-themes` | When the resources directory or the themes are missing, the "Could not find the resources directory" and "No themes found" messages are never shown (stderr is never flushed), so the command exits with status 1 and no output. | Shows both messages. | UD23 |
| `+ssh-cache` | A scoped IPv6 address longer than 255 bytes given as a host (for example `--add=fe80::1%<long scope>`) crashes the command (an integer-cast trap in Zig's IPv6 parser). | Rejects the host as invalid, as it does any scoped IPv6 address. | UD160 |
