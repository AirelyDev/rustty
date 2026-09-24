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
| Closing a terminal | Closing a terminal whose program ignores the hangup signal (or, on macOS, a login shell stuck exiting with unread output) never finishes: Ghostty waits for it forever. | Stops waiting after 10 seconds and finishes closing, leaving the program running. | UD93 |
| Opening a terminal (error path) | If opening a terminal fails after its shell has started (for example when no more threads or file descriptors can be created), the shell is stopped while nothing reads its output, which on macOS can leave it stuck and the stop waiting forever. | Reads and discards the shell's pending output before stopping it. | UD102 |
| Color queries (OSC 4, 10-19) | A sequence that queries about 25 or more colors at once (for example the whole palette) gets no answer at all, and any color changes after the queries in the same sequence are ignored: the answer overflows a small fixed buffer. | Answers every query and applies every change. | UD205 |
| Shell integration (bash) | A bash `command` that repeats `--norc` or `--noprofile` enough times (for example `bash --norc --norc --norc --norc --norc`) makes the terminal fail to open: the flags overflow a 32-byte buffer. | Opens the terminal with shell integration, passing every flag on. | UD204 |
| systemd (Linux) | A `NOTIFY_SOCKET` exactly 108 bytes long crashes Ghostty when it notifies systemd (the terminating NUL is written past the address buffer). | Reports "NOTIFY_SOCKET path is too long!", as for a longer value. | UD202 |
| systemd (Linux) | A `NOTIFY_SOCKET` naming an abstract socket (`@...`), which the code says it supports, is used as a file path named `@...`, so readiness notifications never arrive. | Connects to the abstract socket. | UD203 |
| Paths on Windows | Converting a path or argument that holds the overlong byte sequence `E0 80..9F xx` for a Windows API (for example a `PATH` entry searched for a command) crashes, in Zig's WTF-8 decoder. | Treats it as an invalid path, as it does any other malformed byte sequence. | UD201 |
| Opening URLs | When the program used to open a URL (`xdg-open`, `open`) writes a line to stderr, the thread that collects its output loops forever logging empty lines and never reaps the program. | Logs each line once and reaps the program. | UD200 |
| `+ssh-cache` | A scoped IPv6 address longer than 255 bytes given as a host (for example `--add=fe80::1%<long scope>`) crashes the command (an integer-cast trap in Zig's IPv6 parser). | Rejects the host as invalid, as it does any scoped IPv6 address. | UD160 |
| Fonts (FreeType: Linux, FreeBSD, Windows) | Asking for the name of a font whose family name (its Windows Unicode record) is too long for the buffer it is decoded into crashes, or in a ReleaseFast build writes past that buffer, because the UTF-16 decoder is not told the buffer is too small. Where Ghostty passes a 256-byte buffer (for example when it logs a synthetic bold or italic face) this starts at 86 CJK characters or 257 ASCII characters; elsewhere at 342 CJK characters. | Returns the undecoded name record instead, as Ghostty already does for a name it cannot decode. | UD190 |
| Home directory (Linux, FreeBSD) | When `HOME` is unset and the user database has no home directory, Ghostty asks the shell. That fails with an out-of-memory error when the home path is 361 bytes or longer, although the code allows 512 bytes, so `~/` paths are then not expanded. | Accepts a home path of up to 511 bytes from the shell, as the 512-byte limit intends. | UD210 |
| Temporary directory | `TMPDIR=/` (or `TMP=/`) is turned into an empty path, which Ghostty's own temporary-directory code cannot open. | Keeps `/`. An empty `TMPDIR` is an error, as in Ghostty: no temporary directory is created. | UD211 |
| Working directory (OSC 7) | In the directory a program reports with OSC 7, a malformed escape `%-0` becomes a NUL byte and `%+F` (a `+` and one hex digit) becomes a control character, although every other malformed escape is kept as written (Zig's URI decoder reads the sign as part of a number). | Decodes only `%` followed by two hex digits and keeps `%-0` and `%+F` as written. | UD230 |
