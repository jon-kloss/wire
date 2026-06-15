// Transport shim. The React UI talks to the backend exclusively through
// `invoke(command, args)` and `open(dialogOptions)`. In the Tauri desktop build
// these go over Tauri IPC; in the web playground build they go over HTTP to the
// wire-web server, and `open` is satisfied by an in-app sample gallery instead
// of a native folder picker. The rest of the app is unaware of which is active.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { open as tauriOpen } from "@tauri-apps/plugin-dialog";

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function isWebPlayground(): boolean {
  return !isTauri();
}

export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>
): Promise<T> {
  if (isTauri()) return tauriInvoke<T>(cmd, args);

  const res = await fetch(`/api/${cmd}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    credentials: "same-origin",
    body: JSON.stringify(args ?? {}),
  });
  const text = await res.text();
  if (!res.ok) {
    // Match Tauri's behavior exactly: invoke rejects with the bare error
    // string (not an Error), so `String(err)` renders identically in both
    // builds rather than gaining an "Error: " prefix in the browser.
    throw text || res.statusText;
  }
  return (text ? JSON.parse(text) : undefined) as T;
}

export interface OpenDialogOptions {
  directory?: boolean;
  multiple?: boolean;
  title?: string;
}

// Web-mode folder picker bridge: `open()` dispatches an event the SampleGallery
// component listens for, and resolves once the user picks (or cancels).
const dialogResolvers = new Map<number, (value: string | null) => void>();
let dialogSeq = 0;

export async function open(
  options?: OpenDialogOptions
): Promise<string | string[] | null> {
  if (isTauri()) {
    return tauriOpen(options as Parameters<typeof tauriOpen>[0]);
  }
  return new Promise<string | null>((resolve) => {
    const id = ++dialogSeq;
    dialogResolvers.set(id, resolve);
    window.dispatchEvent(
      new CustomEvent("wire:open-dialog", { detail: { id, options } })
    );
  });
}

export function resolveOpenDialog(id: number, value: string | null): void {
  const resolver = dialogResolvers.get(id);
  if (resolver) {
    dialogResolvers.delete(id);
    resolver(value);
  }
}

export interface SampleInfo {
  label: string;
  description: string;
  path: string;
  kind: "collection" | "project" | "location";
}
