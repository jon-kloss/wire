/* Wire logo + brand components. The accent hook and presets live in accent.ts
   so this file only exports components (Fast Refresh friendly). */

import { ACCENTS, type Accent } from "./accent";

/** Logo mark: "W" drawn as a waveform. Inherits color from currentColor (accent). */
export function WireMark({ size = 18 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      aria-hidden
      style={{ display: "block", flexShrink: 0, color: "var(--accent)" }}
    >
      <path
        d="M5 9L11 23L16 13L21 23L27 9"
        stroke="currentColor"
        strokeWidth={3.2}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="5" cy="9" r="2.7" fill="currentColor" />
      <circle cx="27" cy="9" r="2.7" fill="currentColor" />
    </svg>
  );
}

/** Wordmark: "Wire" with the "i" as a charged accent node. */
export function WireWordmark({ size = 13 }: { size?: number }) {
  return (
    <span
      className="wire-wordmark"
      style={{ fontSize: size, fontWeight: 700, letterSpacing: "-0.03em" }}
    >
      W<span style={{ color: "var(--accent-bright)" }}>i</span>re
    </span>
  );
}

/** Full lockup (mark + wordmark) for the title bar. */
export function WireLockup({
  markSize = 18,
  wordSize = 13,
  gap = 8,
}: {
  markSize?: number;
  wordSize?: number;
  gap?: number;
}) {
  return (
    <span style={{ display: "flex", alignItems: "center", gap }}>
      <WireMark size={markSize} />
      <WireWordmark size={wordSize} />
    </span>
  );
}

/** The accent picker (title bar, right side). */
export function AccentPicker({
  accent,
  onPick,
}: {
  accent: Accent;
  onPick: (a: Accent) => void;
}) {
  return (
    <div className="accent-picker">
      <span className="accent-picker-label">ACCENT</span>
      <div className="accent-picker-swatches">
        {ACCENTS.map(({ key, label, swatch }) => {
          const active = key === accent;
          return (
            <button
              key={key}
              title={label}
              aria-label={`Accent: ${label}`}
              aria-pressed={active}
              className={`accent-swatch ${active ? "active" : ""}`}
              onClick={() => onPick(key)}
              style={{
                background: swatch,
                boxShadow: active
                  ? `0 0 0 2px var(--titlebar), 0 0 0 4px ${swatch}`
                  : "none",
              }}
            />
          );
        })}
      </div>
    </div>
  );
}
