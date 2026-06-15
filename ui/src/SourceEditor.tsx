import { useState, useEffect, useCallback, lazy, Suspense } from "react";
import { invoke } from "./api/invoke";
import type { DriftReport } from "./types";

const MonacoEditor = lazy(() => import("@monaco-editor/react"));

const LANG_BY_EXT: Record<string, string> = {
  py: "python",
  js: "javascript",
  mjs: "javascript",
  jsx: "javascript",
  ts: "typescript",
  tsx: "typescript",
  cs: "csharp",
  java: "java",
  kt: "kotlin",
  go: "go",
  rb: "ruby",
  php: "php",
  json: "json",
  yaml: "yaml",
  yml: "yaml",
  toml: "ini",
  xml: "xml",
  csproj: "xml",
  gradle: "groovy",
  properties: "ini",
  md: "markdown",
};

function languageFor(path: string): string {
  const ext = path.split(".").pop() ?? "";
  return LANG_BY_EXT[ext] ?? "plaintext";
}

interface Props {
  sourceDir: string;
  /** wire dir of the collection generated from this source, opened before
   *  re-checking drift so the check always targets the right project. */
  wireDir: string | null;
  onClose: () => void;
  onDrift: (report: DriftReport) => void;
}

/**
 * In-browser editor for a scanned project's source files. Lets a visitor change
 * a route handler, save it, and re-run drift detection — turning the codebase
 * features into a hands-on demo. Web playground only.
 */
export function SourceEditor({ sourceDir, wireDir, onClose, onDrift }: Props) {
  const [files, setFiles] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadFile = useCallback(
    async (path: string) => {
      try {
        const text = await invoke<string>("read_source_file", { sourceDir, path });
        setSelected(path);
        setContent(text);
        setDirty(false);
        setStatus(null);
      } catch (err) {
        setError(typeof err === "string" ? err : String(err));
      }
    },
    [sourceDir]
  );

  useEffect(() => {
    invoke<string[]>("list_source_files", { sourceDir })
      .then((list) => {
        setFiles(list);
        if (list.length > 0) loadFile(list[0]);
      })
      .catch((err) => setError(typeof err === "string" ? err : String(err)));
  }, [sourceDir, loadFile]);

  const save = useCallback(async () => {
    if (!selected) return;
    setSaving(true);
    setError(null);
    try {
      await invoke("save_source_file", { sourceDir, path: selected, content });
      setDirty(false);
      setStatus("Saved");
    } catch (err) {
      setError(typeof err === "string" ? err : String(err));
    } finally {
      setSaving(false);
    }
  }, [sourceDir, selected, content]);

  const checkDrift = useCallback(async () => {
    setError(null);
    try {
      // Ensure the backend has this source's collection open before checking,
      // rather than relying on whatever the caller happened to leave open.
      if (wireDir) await invoke("open_collection", { wireDir });
      const report = await invoke<DriftReport>("check_drift");
      const safe = report ?? { items: [], new_count: 0, stale_count: 0, changed_count: 0 };
      onDrift(safe);
      const total = safe.new_count + safe.stale_count + safe.changed_count;
      setStatus(
        total === 0
          ? "No drift detected"
          : `Drift: +${safe.new_count} new, -${safe.stale_count} stale, ~${safe.changed_count} changed`
      );
    } catch (err) {
      setError(typeof err === "string" ? err : String(err));
    }
  }, [wireDir, onDrift]);

  return (
    <div className="source-editor-overlay" onClick={onClose}>
      <div className="source-editor" onClick={(e) => e.stopPropagation()}>
        <div className="source-editor-header">
          <h2>Edit source &amp; re-check drift</h2>
          <button className="source-editor-close" onClick={onClose}>
            ✕
          </button>
        </div>
        <p className="source-editor-hint">
          Change a route handler (add or remove an endpoint), save, then re-check
          drift to see Wire detect the difference against the generated collection.
        </p>
        <div className="source-editor-body">
          <div className="source-file-list">
            {files.length === 0 && <div className="source-empty">No source files.</div>}
            {files.map((f) => (
              <button
                key={f}
                className={`source-file-item ${f === selected ? "selected" : ""}`}
                onClick={() => {
                  if (f === selected) return;
                  if (dirty && !window.confirm("Discard unsaved changes?")) return;
                  loadFile(f);
                }}
              >
                {f}
              </button>
            ))}
          </div>
          <div className="source-editor-pane">
            {selected ? (
              <Suspense fallback={<div className="source-empty">Loading editor…</div>}>
                <MonacoEditor
                  height="100%"
                  theme="vs-dark"
                  language={languageFor(selected)}
                  value={content}
                  onChange={(v) => {
                    setContent(v ?? "");
                    setDirty(true);
                    setStatus(null);
                  }}
                  options={{ minimap: { enabled: false }, fontSize: 13 }}
                />
              </Suspense>
            ) : (
              <div className="source-empty">Select a file to edit.</div>
            )}
          </div>
        </div>
        <div className="source-editor-footer">
          {error && <span className="source-editor-error">{error}</span>}
          {status && !error && <span className="source-editor-status">{status}</span>}
          <span className="source-editor-spacer" />
          <button onClick={save} disabled={!selected || !dirty || saving}>
            {saving ? "Saving…" : dirty ? "Save" : "Saved"}
          </button>
          <button className="source-editor-primary" onClick={checkDrift} disabled={dirty}>
            Re-check drift
          </button>
        </div>
      </div>
    </div>
  );
}
