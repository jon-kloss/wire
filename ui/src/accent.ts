/* Accent theming model. tokens.css defines the accent vars on :root and
   per-theme overrides under :root[data-accent="ember|signal|pulse"]; we flip
   the theme by setting document.documentElement.dataset.accent (persisted to
   localStorage). No tree re-render is needed to recolor — every component
   references var(--accent*), so the CSS cascade does the work. */

import { useCallback, useEffect, useState } from "react";

export type Accent = "ember" | "signal" | "pulse";

export const ACCENTS: { key: Accent; label: string; swatch: string }[] = [
  { key: "ember", label: "Ember", swatch: "#ff6a4d" },
  { key: "signal", label: "Signal", swatch: "#b8ec4f" },
  { key: "pulse", label: "Pulse", swatch: "#1fc6e8" },
];

const STORAGE_KEY = "wire.accent";

/** Reads/writes <html data-accent> and persists the choice. */
export function useAccent() {
  const [accent, setAccentState] = useState<Accent>(() => {
    const saved =
      typeof localStorage !== "undefined"
        ? (localStorage.getItem(STORAGE_KEY) as Accent | null)
        : null;
    return saved && ACCENTS.some((a) => a.key === saved) ? saved : "ember";
  });

  useEffect(() => {
    document.documentElement.dataset.accent = accent;
    try {
      localStorage.setItem(STORAGE_KEY, accent);
    } catch {
      /* storage may be unavailable */
    }
  }, [accent]);

  const setAccent = useCallback((a: Accent) => setAccentState(a), []);
  return { accent, setAccent };
}
