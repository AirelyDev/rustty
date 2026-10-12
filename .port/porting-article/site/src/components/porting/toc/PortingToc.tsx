"use client";

import { ChevronDown } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import useReducedMotion from "../ui/useReducedMotion";
import s from "./PortingToc.module.css";

/** One numbered section entry (a level-2 heading of the article). */
export type TocItem = { id: string; title: string; number: number };

/** A labelled run of consecutive sections. */
export type TocGroup = { key: string; label: string; items: TocItem[] };

/** BAR_SELECTOR finds the phone contents bar (its height is part of the reading line). */
const BAR_SELECTOR = "[data-porting-tocbar]";

/** READING_LINE_SLACK is how far below the sticky chrome a heading may sit and still count as current. */
const READING_LINE_SLACK = 96;

/** Formats a section number as two digits (01–99). */
function pad(n: number): string {
  return n.toString().padStart(2, "0");
}

/**
 * Returns the id of the current section: the last heading whose top has
 * crossed the reading line (sticky header and phone bar, plus some slack), or
 * the last heading once the page is scrolled to the bottom. Null above the
 * first heading.
 */
function useActiveSection(ids: string[]): string | null {
  const [active, setActive] = useState<string | null>(null);
  const key = ids.join("\n");
  useEffect(() => {
    const list = key.split("\n");
    let frame = 0;
    const update = () => {
      frame = 0;
      const header =
        Number.parseFloat(
          getComputedStyle(document.documentElement).getPropertyValue(
            "--header-height",
          ),
        ) || 72;
      const bar =
        document.querySelector<HTMLElement>(BAR_SELECTOR)?.offsetHeight ?? 0;
      const line = header + bar + READING_LINE_SLACK;
      let current: string | null = null;
      for (const id of list) {
        const el = document.getElementById(id);
        if (!el) continue;
        const target = el.querySelector("h1, h2, h3, h4, h5, h6") ?? el;
        if (target.getBoundingClientRect().top <= line) current = id;
        else break;
      }
      const doc = document.documentElement;
      if (
        current !== null &&
        window.innerHeight + window.scrollY >= doc.scrollHeight - 2
      ) {
        current = list[list.length - 1];
      }
      setActive(current);
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);
    window.addEventListener("hashchange", schedule);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      window.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
      window.removeEventListener("hashchange", schedule);
    };
  }, [key]);
  return active;
}

/** Flattens the groups into section ids in article order. */
function useIds(groups: TocGroup[]): string[] {
  return useMemo(
    () => groups.flatMap((g) => g.items.map((i) => i.id)),
    [groups],
  );
}

/**
 * Renders the grouped, numbered list. Every row (group labels and entries)
 * carries one segment of the left rail: rows before the current entry are
 * filled (reading progress), the current entry carries the copper marker.
 */
function TocList({
  groups,
  activeId,
  idPrefix,
  onSelect,
}: {
  groups: TocGroup[];
  activeId: string | null;
  idPrefix: string;
  onSelect?: () => void;
}) {
  const activeNumber =
    groups.flatMap((g) => g.items).find((i) => i.id === activeId)?.number ?? 0;
  return (
    <div className={s.list}>
      {groups.map((g) => {
        const labelId = `${idPrefix}-group-${g.key}`;
        const first = g.items[0]?.number ?? Number.POSITIVE_INFINITY;
        return (
          <div key={g.key} className={s.group}>
            <p
              id={labelId}
              className={s.groupLabel}
              data-state={activeNumber >= first ? "read" : undefined}
            >
              {g.label}
            </p>
            <ul className={s.items} aria-labelledby={labelId}>
              {g.items.map((item) => {
                const state =
                  item.number === activeNumber
                    ? "active"
                    : item.number < activeNumber
                      ? "read"
                      : undefined;
                return (
                  <li key={item.id} className={s.item} data-state={state}>
                    <a
                      href={`#${item.id}`}
                      aria-current={state === "active" ? "location" : undefined}
                      onClick={onSelect}
                    >
                      <span className={s.num}>{pad(item.number)}</span>
                      <span className={s.title}>{item.title}</span>
                    </a>
                  </li>
                );
              })}
            </ul>
          </div>
        );
      })}
    </div>
  );
}

/** Desktop table of contents: sticky beside the article, current entry kept in view. */
export function PortingSideToc({
  groups,
  className,
}: {
  groups: TocGroup[];
  className?: string;
}) {
  const ids = useIds(groups);
  const activeId = useActiveSection(ids);
  const reduced = useReducedMotion();
  const navRef = useRef<HTMLElement>(null);
  useEffect(() => {
    const nav = navRef.current;
    if (!nav || !activeId || nav.offsetParent === null) return;
    const link = nav.querySelector<HTMLElement>('[aria-current="location"]');
    if (!link) return;
    const navRect = nav.getBoundingClientRect();
    const linkRect = link.getBoundingClientRect();
    const delta =
      linkRect.top - navRect.top - (navRect.height - linkRect.height) / 2;
    nav.scrollBy({ top: delta, behavior: reduced ? "auto" : "smooth" });
  }, [activeId, reduced]);
  return (
    <nav
      ref={navRef}
      aria-label="Contents"
      className={`${s.side} ${className ?? ""}`}
    >
      <TocList groups={groups} activeId={activeId} idPrefix="toc" />
    </nav>
  );
}

/**
 * Phone and tablet table of contents: a sticky bar under the site header
 * naming the current section; it discloses the full list. Escape, a tap
 * outside or choosing an entry closes it and returns focus to the button.
 */
export function PortingPhoneToc({ groups }: { groups: TocGroup[] }) {
  const ids = useIds(groups);
  const activeId = useActiveSection(ids);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLElement>(null);
  const active = groups.flatMap((g) => g.items).find((i) => i.id === activeId);

  const close = (refocus: boolean) => {
    setOpen(false);
    if (refocus) buttonRef.current?.focus({ preventScroll: true });
  };

  useEffect(() => {
    if (!open) return;
    const panel = panelRef.current;
    const current = panel?.querySelector<HTMLElement>(
      '[aria-current="location"]',
    );
    if (panel && current) {
      panel.scrollTop =
        current.offsetTop - panel.clientHeight / 2 + current.offsetHeight / 2;
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close(true);
      }
    };
    const onPointer = (e: PointerEvent) => {
      const root = rootRef.current;
      if (root && !root.contains(e.target as Node)) {
        close(root.contains(document.activeElement));
      }
    };
    const onFocusOut = (e: FocusEvent) => {
      const root = rootRef.current;
      const next = e.relatedTarget as Node | null;
      if (root && next && !root.contains(next)) setOpen(false);
    };
    const root = rootRef.current;
    document.addEventListener("keydown", onKey);
    document.addEventListener("pointerdown", onPointer);
    root?.addEventListener("focusout", onFocusOut);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("pointerdown", onPointer);
      root?.removeEventListener("focusout", onFocusOut);
    };
  }, [open]);

  return (
    <div ref={rootRef} className={s.bar} data-porting-tocbar="">
      <div className={s.barInner}>
        <button
          ref={buttonRef}
          type="button"
          className={s.barButton}
          aria-expanded={open}
          aria-controls="porting-toc-panel"
          onClick={() => setOpen((v) => !v)}
        >
          {active ? (
            <>
              <span className={s.srOnly}>Contents, current section: </span>
              <span className={s.barNum}>{pad(active.number)}</span>
              <span className={s.barSep} aria-hidden="true">
                ·
              </span>
              <span className={s.barTitle}>{active.title}</span>
            </>
          ) : (
            <span className={s.barTitle}>Contents</span>
          )}
          <ChevronDown
            className={s.chevron}
            data-open={open ? "true" : undefined}
            size={16}
            aria-hidden="true"
          />
        </button>
        <nav
          ref={panelRef}
          id="porting-toc-panel"
          aria-label="Contents"
          className={s.panel}
          hidden={!open}
        >
          <TocList
            groups={groups}
            activeId={activeId}
            idPrefix="toc-phone"
            onSelect={() => {
              setOpen(false);
              // After the fragment navigation, which resets focus to the page.
              setTimeout(
                () => buttonRef.current?.focus({ preventScroll: true }),
                0,
              );
            }}
          />
        </nav>
      </div>
    </div>
  );
}
