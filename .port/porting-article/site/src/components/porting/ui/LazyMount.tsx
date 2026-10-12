"use client";

import {
  Component,
  type ComponentType,
  lazy,
  type ReactNode,
  Suspense,
  useMemo,
} from "react";
import { useInView } from "react-intersection-observer";
import s from "./ui.module.css";

/** ROOT_MARGIN starts loading a widget once it is within 600 px of the viewport. */
const ROOT_MARGIN = "600px 0px";

/** Props for LazyMount: what to render once near the viewport. */
type LazyMountProps = {
  /** Reserved height in px for the skeleton and the loaded content. */
  minHeight?: number;
  /**
   * Dynamic import of the component to render, used when given. It must be
   * referentially stable (a module-level function), or the lazy component is
   * recreated and remounted on every change of identity.
   */
  load?: () => Promise<{ default: ComponentType }>;
  /** Content to render when no load function is given (e.g. a client WidgetMount). */
  children?: ReactNode;
};

/** Props for MountErrorBoundary. */
type BoundaryProps = { children: ReactNode };

/** State for MountErrorBoundary: whether a descendant has thrown. */
type BoundaryState = { failed: boolean };

/**
 * Catches errors from a widget body (a failed chunk load or a render error)
 * so one broken widget shows a placeholder instead of taking down the article.
 */
class MountErrorBoundary extends Component<BoundaryProps, BoundaryState> {
  /** Initial state: nothing has failed. */
  state: BoundaryState = { failed: false };

  /** Switches to the fallback after a descendant throws. */
  static getDerivedStateFromError(): BoundaryState {
    return { failed: true };
  }

  /** Renders children, or the placeholder once a descendant has failed. */
  render() {
    if (this.state.failed) {
      return <div className={s.pending}>Interactive view unavailable</div>;
    }
    return this.props.children;
  }
}

/**
 * Renders a skeleton until the container is within 600 px of the viewport
 * (or immediately where IntersectionObserver is missing), then renders the
 * lazily loaded component (or children) under Suspense and an error boundary.
 * The skeleton is also what the static HTML contains.
 */
export default function LazyMount({
  minHeight,
  load,
  children,
}: LazyMountProps) {
  const { ref, inView } = useInView({
    rootMargin: ROOT_MARGIN,
    triggerOnce: true,
    fallbackInView: true,
  });
  const Loaded = useMemo(() => (load ? lazy(load) : null), [load]);
  const skeleton = <div className={s.skeleton} aria-hidden="true" />;
  return (
    <div
      ref={ref}
      className={s.lazy}
      style={minHeight ? { minHeight } : undefined}
    >
      {inView ? (
        <MountErrorBoundary>
          <Suspense fallback={skeleton}>
            {Loaded ? <Loaded /> : children}
          </Suspense>
        </MountErrorBoundary>
      ) : (
        skeleton
      )}
    </div>
  );
}
