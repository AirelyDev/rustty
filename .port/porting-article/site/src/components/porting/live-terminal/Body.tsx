"use client";

import {
  Component,
  type ComponentProps,
  type ComponentType,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { ENGINES, type EngineId } from "./engines";
import s from "./LiveTerminal.module.css";
import { OutputLog, OutputRouter } from "./session";

/** TerminalView's module type; a type query, so Body never imports it statically. */
type TerminalModule = typeof import("./TerminalView");
/** Props of TerminalView. */
type ViewProps = ComponentProps<TerminalModule["default"]>;
/** The session's shell, created once by TerminalView's startShell. */
type Shell = Awaited<ReturnType<TerminalModule["startShell"]>>;
/** What a successful start yields: the view component and the running shell. */
type Session = { View: ComponentType<ViewProps>; shell: Shell };

/**
 * Loads TerminalView on demand, so wterm, just-bash and the wasm are separate
 * chunks fetched only once the demo mounts, then starts the session's one
 * shell, writing into router. The shell outlives every engine switch.
 */
function loadSession(router: OutputRouter): Promise<Session> {
  return import("./TerminalView").then(async (m) => ({
    View: m.default,
    shell: await m.startShell(router),
  }));
}

/** PaneState is one terminal pane's status: loading its wasm, drawing, or failed. */
type PaneState = "loading" | "ready" | "failed";

/** PaneMap holds a value for each engine's pane. */
type PaneMap<T> = Record<EngineId, T>;

/** Creates a PaneMap with the same value for every engine. */
function everyPane<T>(value: T): PaneMap<T> {
  return { rustty: value, ghostty: value };
}

/** WTERM_HREF is where a visitor can try the terminal front end when the demo cannot start here. */
const WTERM_HREF = "https://wterm.dev";

/**
 * ViewBoundary catches a render-time throw from the terminal view and reports
 * it, so Body shows its failed panel instead of the card's generic fallback.
 */
class ViewBoundary extends Component<
  { onError: (error: unknown) => void; children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false };

  /** Marks the boundary failed so it stops rendering the broken view. */
  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  /** Hands the error to Body, which switches to the failed phase. */
  componentDidCatch(error: unknown): void {
    this.props.onError(error);
  }

  /** Renders the view until it has thrown. */
  render(): ReactNode {
    return this.state.failed ? null : this.props.children;
  }
}

/**
 * LiveTerminal is the /porting demo: wterm + just-bash, with the session drawn
 * by both engines side by side. Typing in either pane goes to the one shell,
 * and its output goes to both panes.
 */
export default function LiveTerminal() {
  const [bootFailed, setBootFailed] = useState(false);
  const [panes, setPanes] = useState<PaneMap<PaneState>>(everyPane("loading"));
  // Bumped to remount one pane's terminal after it failed.
  const [attempts, setAttempts] = useState<PaneMap<number>>(everyPane(0));
  const [session, setSession] = useState<Session | null>(null);
  // One log and router for the whole session; the lazy initializer keeps a
  // single pair under StrictMode.
  const [router] = useState(() => new OutputRouter(new OutputLog()));
  const booting = useRef<Promise<Session> | null>(null);

  const setPane = useCallback(
    (id: EngineId, state: PaneState) =>
      setPanes((p) => (p[id] === state ? p : { ...p, [id]: state })),
    [],
  );
  // Stable per-pane callbacks: TerminalView remounts when they change.
  const handlers = useMemo(
    () =>
      Object.fromEntries(
        ENGINES.map((e) => [
          e.id,
          {
            ready: () => setPane(e.id, "ready"),
            error: (err: unknown) => {
              // The loader's message is written for developers; keep it in the console.
              console.error(`live terminal (${e.id}) failed`, err);
              setPane(e.id, "failed");
            },
          },
        ]),
      ) as PaneMap<{ ready: () => void; error: (e: unknown) => void }>,
    [setPane],
  );

  // The demo starts as soon as it mounts: it loads TerminalView and the shell
  // once. StrictMode runs this twice; the second run reuses the first's promise.
  useEffect(() => {
    booting.current ??= loadSession(router);
    booting.current.then(setSession, (e: unknown) => {
      booting.current = null;
      console.error("live terminal failed to start", e);
      setBootFailed(true);
    });
  }, [router]);

  if (bootFailed) {
    return (
      <div className={s.gate} role="alert">
        <p className={s.note}>
          The live terminal could not start in this browser. The terminal front
          end is{" "}
          <a className={s.link} href={WTERM_HREF}>
            wterm
          </a>
          .
        </p>
        <button
          type="button"
          className={s.retry}
          onClick={() => {
            // Turbopack's runtime caches a failed chunk load for the life of
            // the page, so a reload is the only way to fetch TerminalView or
            // just-bash again; the reloaded page starts on its own.
            window.location.reload();
          }}
        >
          Try again
        </button>
      </div>
    );
  }
  return (
    <div className={s.panes}>
      {ENGINES.map((e) => {
        const state = panes[e.id];
        const status =
          state === "ready"
            ? "Ready"
            : state === "failed"
              ? "Failed"
              : "Starting…";
        return (
          <section
            key={e.id}
            className={s.pane}
            data-engine={e.id}
            data-phase={state}
            aria-label={e.label}
          >
            <div className={s.bar}>
              <span className={s.dot} aria-hidden="true" />
              <span className={s.text}>
                <span className={s.name}>{e.name}</span>
                <span className={s.sub}>{e.sub}</span>
              </span>
              <span className={s.status} aria-live="polite">
                {status}
              </span>
            </div>
            {state === "failed" ? (
              <div className={s.paneError} role="alert">
                <p className={s.note}>This engine could not start here.</p>
                <button
                  type="button"
                  className={s.retry}
                  onClick={() => {
                    setAttempts((a) => ({ ...a, [e.id]: a[e.id] + 1 }));
                    setPane(e.id, "loading");
                  }}
                >
                  Try again
                </button>
              </div>
            ) : session ? (
              <ViewBoundary
                key={`${e.id}-${attempts[e.id]}`}
                onError={handlers[e.id].error}
              >
                <session.View
                  engine={e.id}
                  shell={session.shell}
                  router={router}
                  onReady={handlers[e.id].ready}
                  onError={handlers[e.id].error}
                />
              </ViewBoundary>
            ) : (
              <div className={s.loading}>Starting the terminal…</div>
            )}
          </section>
        );
      })}
    </div>
  );
}
