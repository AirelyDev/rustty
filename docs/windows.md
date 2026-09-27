# Rustty on Windows

At the Ghostty version Rustty ports (pin `20abdb50a`), Ghostty has no Windows terminal
window, and neither does Rustty.

## What `rustty.exe` is

On Windows, `rustty.exe` is the **helper CLI**, the same program Ghostty's `ghostty.exe` is
at the pin. Ghostty picks its app runtime per OS (`src/apprt/runtime.zig`): GTK on Linux and
FreeBSD, and `none` everywhere else. With `none` there is no GUI. The macOS GUI is a separate
Swift app, and Windows has no GUI at all. The helper CLI runs `+<action>` commands
(`+version`, `+list-fonts`, `+show-config`, `+validate-config`, ...) and exits. With no
action it prints a short usage text and exits.

The executable is built as a Windows GUI-subsystem program, as Ghostty builds its own
(`src/build/GhosttyExe.zig`, with the manifest, icon and version resources of
`dist/windows/rustty.rc`). The GUI subsystem only means that Windows gives the program no
console window.

## What you see when you run it

- **Double-clicking it**, or starting it with `Start-Process`: nothing happens. No window
  opens. The program has nowhere to write, so it exits at once with status 1. Ghostty's exe
  does the same.
- **Typing `rustty +version` at a PowerShell or cmd prompt**: nothing is printed either.
  A shell does not wait for a GUI-subsystem program, and the program cannot write to the
  console it was started from. Ghostty's exe does the same.
- **To see the output, redirect or pipe it:**

  ```powershell
  rustty +version | Out-String
  rustty +list-fonts | Out-String
  cmd /c "rustty +show-config --default > config.txt"
  ```

  In PowerShell 5.1, `rustty +version > out.txt` leaves the file empty, because PowerShell
  does not wait for a GUI-subsystem program there. Pipe the output, or redirect it through
  `cmd /c` as shown above.

## How this is tested

`.port/tools/windows/exe-differential.ps1` runs `rustty.exe` and Ghostty's exe (built at
the pin on Windows) over the whole CLI matrix and the "opened" starts, and compares stdout,
stderr and exit status. The only differences left are the listed ones: Ghostty bugs that
Rustty fixes (`docs/ghostty-bug-fixes.md`), and places where the Rustty name itself changes
the output. The CI Windows leg runs the same script against the reference's captured
output (`.port/tools/windows/golden/`). The script's header explains how the reference exe
is built and how to regenerate the captures.
