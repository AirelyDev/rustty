"use client";

import "@wterm/dom/css";
import type { BashShell } from "@wterm/just-bash";
import { useEffect, useRef } from "react";
import { DEMO_SCRIPTS } from "./demo-scripts";
import { ENGINES, type EngineId } from "./engines";
import s from "./LiveTerminal.module.css";
import { ENV, GREETING, type OutputRouter, prompt, seedFiles } from "./session";

/**
 * Creates the session's one shell and attaches it to router. Call it once per
 * session: BashShell.attach builds a fresh Bash (files, cwd and history reset)
 * and prints the greeting, so engine switches must reuse the shell it returns.
 */
export async function startShell(router: OutputRouter): Promise<BashShell> {
  const { BashShell } = await import("@wterm/just-bash");
  const shell = new BashShell({
    files: seedFiles(DEMO_SCRIPTS),
    env: ENV,
    cwd: ENV.HOME,
    greeting: GREETING,
    prompt,
  });
  await shell.attach((chunk: string) => router.write(chunk));
  // BashShell seeds files as plain text (mode 0644); make the demo
  // scripts executable so ./demo/<name>.sh runs.
  const fs = shell.bash?.fs;
  if (fs) {
    await Promise.all(
      Object.keys(DEMO_SCRIPTS).map((path) => fs.chmod(path, 0o755)),
    );
  }
  return shell;
}

/** Props for TerminalView. */
type Props = {
  /** Engine whose wasm draws the terminal. */
  engine: EngineId;
  /** The session's shell, shared by both engines' terminals. */
  shell: BashShell;
  /** Routes the shell's output to every terminal; its log is replayed into each new one. */
  router: OutputRouter;
  /** Called once the terminal is interactive. */
  onReady: () => void;
  /** Called when loading or running the terminal fails. */
  onError: (error: unknown) => void;
};

/**
 * Mounts a wterm terminal driven by the chosen engine and connects it to the
 * session's shell. Both engines' terminals are mounted at once, each attached
 * to the router under its engine id. It is the only module that imports wterm
 * or just-bash, and it is only loaded through a dynamic import once the demo
 * mounts.
 */
export default function TerminalView({
  engine,
  shell,
  router,
  onReady,
  onError,
}: Props) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let disposed = false;
    let attached = false;
    let term: { destroy: () => void } | null = null;
    let core: { dispose: () => void } | null = null;

    (async () => {
      const [{ WTerm }, { GhosttyCore }] = await Promise.all([
        import("@wterm/dom"),
        import("@wterm/ghostty"),
      ]);
      const choice = ENGINES.find((e) => e.id === engine);
      if (!choice) throw new Error(`unknown engine: ${engine}`);
      const loaded = await GhosttyCore.load({ wasmPath: choice.wasmPath });
      if (disposed || !host.current) {
        loaded.dispose();
        return;
      }
      core = loaded;

      const reduced = window.matchMedia(
        "(prefers-reduced-motion: reduce)",
      ).matches;
      const t = new WTerm(host.current, {
        core: loaded,
        cursorBlink: reduced ? false : undefined,
        onData: (data: string) => {
          // Output being delivered can make a terminal answer a query; that
          // reply is not typing, and with two terminals it would arrive twice.
          if (router.writing) return;
          shell.handleInput(data).catch((e: unknown) => {
            if (!disposed) onError(e);
          });
        },
      });
      term = t;
      await t.init();
      if (disposed) return;

      // Attaching replays the session so far (including output written while
      // this terminal was not attached), then new output arrives live.
      router.attach(engine, (chunk: string) => t.write(chunk));
      attached = true;
      onReady();
    })().catch((e: unknown) => {
      if (!disposed) onError(e);
    });

    return () => {
      disposed = true;
      if (attached) router.detach(engine);
      term?.destroy();
      core?.dispose();
      term = null;
      core = null;
    };
  }, [engine, shell, router, onReady, onError]);

  return <div ref={host} className={s.screen} />;
}
