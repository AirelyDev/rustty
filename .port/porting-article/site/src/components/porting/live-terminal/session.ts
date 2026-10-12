/** ENV is the shell environment the demo starts with. */
export const ENV: Record<string, string> = {
  SHELL: "/bin/bash",
  TERM: "xterm-256color",
  COLORTERM: "truecolor",
  USER: "user",
  HOME: "/home/user",
};

/**
 * GREETING is printed once when a session starts, and every attached terminal
 * draws it (the Rust engine and the Zig engine).
 */
export const GREETING: string[] = [
  "\x1b[1mRustty, in your browser.\x1b[0m",
  "One shell, drawn by two engines:",
  "Rust and Zig, at the same time.",
  "Type in either one. Try:",
  "  \x1b[36mls demo\x1b[0m",
  "  \x1b[36m./demo/truecolor.sh\x1b[0m",
  "  \x1b[36mcat README.md\x1b[0m",
  "",
];

/** Returns the colored shell prompt for cwd, with the home directory shown as "~". */
export function prompt(cwd: string): string {
  const shown = cwd.replace(/^\/home\/user/, "~") || "/";
  return `\x1b[1;32muser@rustty\x1b[0m:\x1b[1;34m${shown}\x1b[0m$ `;
}

/**
 * Returns the virtual filesystem the session starts with (absolute paths):
 * a README plus the given demo scripts. Callers pass DEMO_SCRIPTS.
 */
export function seedFiles(
  scripts: Record<string, string>,
): Record<string, string> {
  const files: Record<string, string> = {
    "/home/user/README.md":
      "# Rustty demo\n\nA bash-like shell (just-bash) running in your browser,\ndrawn by the terminal engine you pick above.\n\nRun the scripts in ./demo to exercise the engine.\n",
  };
  for (const [path, text] of Object.entries(scripts)) files[path] = text;
  return files;
}

/**
 * OutputLog keeps the bytes written to the terminal, bounded, so a switched
 * engine can be replayed to the same screen. It drops whole oldest chunks.
 *
 * A chunk is one write() call of the shell (a prompt, an echoed keystroke, a
 * command's whole stdout or stderr), and each carries complete escape
 * sequences, so a truncated log never starts inside one. It can start
 * mid-screen, though, and state set by a dropped chunk (an SGR, the alternate
 * screen, the title) is not restored by the replay.
 */
export class OutputLog {
  private chunks: string[] = [];
  private size = 0;
  readonly maxBytes: number;

  /** Creates a log that holds at most maxBytes UTF-16 code units. */
  constructor(maxBytes = 512 * 1024) {
    this.maxBytes = maxBytes;
  }

  /** Appends a chunk, dropping the oldest chunks while over the limit. */
  push(chunk: string): void {
    this.chunks.push(chunk);
    this.size += chunk.length;
    while (this.size > this.maxBytes && this.chunks.length > 1) {
      this.size -= this.chunks.shift()?.length ?? 0;
    }
  }

  /** Returns everything still held, in order. */
  text(): string {
    return this.chunks.join("");
  }

  /** Empties the log. */
  clear(): void {
    this.chunks = [];
    this.size = 0;
  }
}

/**
 * OutputRouter forwards shell output to every attached terminal and records it
 * in the replay log. The shell writes to it for the whole session; terminals
 * come and go (a failed pane is remounted), and output written while none is
 * attached is logged for the next one. Each terminal is attached under an id.
 *
 * While a chunk or a replay is being delivered, `writing` is true. A terminal
 * that answers a query (cursor position, device attributes) does so inside
 * that call, so input arriving then is a terminal reply, not typing; the view
 * drops it instead of feeding it to the shell, which keeps two terminals from
 * answering the same query twice.
 */
export class OutputRouter {
  private readonly targets = new Map<string, (chunk: string) => void>();
  private readonly log: OutputLog;
  private delivering = 0;

  /** Creates a router that records into log. */
  constructor(log: OutputLog) {
    this.log = log;
  }

  /** True while output is being delivered to terminals. */
  get writing(): boolean {
    return this.delivering > 0;
  }

  /**
   * Points output at terminal id's write function (replacing an earlier one for
   * id) and first delivers the session so far, including output written while
   * it was detached. Both happen in the same task, so no chunk falls between them.
   */
  attach(id: string, write: (chunk: string) => void): void {
    this.targets.set(id, write);
    const replay = this.log.text();
    if (replay) this.deliver(write, replay);
  }

  /** Stops forwarding to terminal id (it was destroyed). */
  detach(id: string): void {
    this.targets.delete(id);
  }

  /**
   * Records chunk and forwards it to every attached terminal. One terminal
   * throwing does not keep the others from receiving it; the first error is
   * rethrown after all have been given the chunk.
   */
  write(chunk: string): void {
    this.log.push(chunk);
    let failure: { error: unknown } | null = null;
    for (const write of [...this.targets.values()]) {
      try {
        this.deliver(write, chunk);
      } catch (error) {
        failure ??= { error };
      }
    }
    if (failure) throw failure.error;
  }

  /** Calls write with text, flagging the call as output delivery. */
  private deliver(write: (chunk: string) => void, text: string): void {
    this.delivering++;
    try {
      write(text);
    } finally {
      this.delivering--;
    }
  }
}
