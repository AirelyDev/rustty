"use client";

import { useEffect, useState } from "react";

/** REDUCED_MOTION_QUERY is the media query for the user's reduced-motion preference. */
const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

/** Returns true when the user prefers reduced motion; tracks changes to the preference. */
export default function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(
    () =>
      typeof window !== "undefined" &&
      window.matchMedia(REDUCED_MOTION_QUERY).matches,
  );
  useEffect(() => {
    const mq = window.matchMedia(REDUCED_MOTION_QUERY);
    const onChange = () => setReduced(mq.matches);
    onChange();
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return reduced;
}
