import { useEffect, useState, useCallback } from "react";
import {
  invoke,
  resolveOpenDialog,
  isWebPlayground,
  type SampleInfo,
} from "./api/invoke";

interface DialogState {
  id: number;
  kind: SampleInfo["kind"];
}

/**
 * Web-playground replacement for the desktop native folder picker. Listens for
 * the `wire:open-dialog` event emitted by the `open()` shim, shows the relevant
 * bundled samples, and resolves the pending `open()` promise with the chosen
 * sandbox path (or null when cancelled). Renders nothing in the desktop build.
 */
export function SampleGallery() {
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [samples, setSamples] = useState<SampleInfo[]>([]);

  useEffect(() => {
    if (!isWebPlayground()) return;
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail as {
        id: number;
        options?: { title?: string };
      };
      const title = detail.options?.title?.toLowerCase() ?? "";
      const kind: SampleInfo["kind"] = title.includes("scan")
        ? "project"
        : title.includes("new collection")
          ? "location"
          : "collection";
      setDialog({ id: detail.id, kind });
      invoke<SampleInfo[]>("list_samples")
        .then(setSamples)
        .catch(() => setSamples([]));
    };
    window.addEventListener("wire:open-dialog", handler);
    return () => window.removeEventListener("wire:open-dialog", handler);
  }, []);

  const pick = useCallback(
    (path: string | null) => {
      if (dialog) resolveOpenDialog(dialog.id, path);
      setDialog(null);
      setSamples([]);
    },
    [dialog]
  );

  if (!dialog) return null;

  const shown = samples.filter((s) => s.kind === dialog.kind);
  const heading =
    dialog.kind === "project"
      ? "Pick a sample project to scan"
      : dialog.kind === "location"
        ? "Pick a location for the new collection"
        : "Open a sample collection";

  return (
    <div className="sample-gallery-overlay" onClick={() => pick(null)}>
      <div className="sample-gallery" onClick={(e) => e.stopPropagation()}>
        <h2>{heading}</h2>
        <p className="sample-gallery-sub">
          This is a sandboxed playground — pick a bundled sample to explore.
        </p>
        <div className="sample-list">
          {shown.length === 0 && (
            <div className="sample-empty">No samples available.</div>
          )}
          {shown.map((s) => (
            <button
              key={s.path}
              className="sample-item"
              onClick={() => pick(s.path)}
            >
              <span className="sample-label">{s.label}</span>
              <span className="sample-desc">{s.description}</span>
            </button>
          ))}
        </div>
        <div className="sample-gallery-actions">
          <button onClick={() => pick(null)}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
