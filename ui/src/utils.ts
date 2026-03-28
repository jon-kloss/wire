import type { IpcRequestEntry } from "./types";

/** Group flat request list into a folder tree */
export interface TreeNode {
  name: string;
  /** Leaf nodes have a request entry */
  entry?: IpcRequestEntry;
  children: Map<string, TreeNode>;
}

export function buildTree(
  requests: IpcRequestEntry[],
  basePath: string
): TreeNode {
  const root: TreeNode = { name: "requests", children: new Map() };

  for (const entry of requests) {
    // Make path relative to the collection's requests/ dir
    let rel = entry.path;
    const requestsPrefix = basePath + "/requests/";
    if (rel.startsWith(requestsPrefix)) {
      rel = rel.slice(requestsPrefix.length);
    }
    const parts = rel.split("/");
    let current = root;
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      if (i === parts.length - 1) {
        // Leaf — the request file
        current.children.set(part, {
          name: entry.name,
          entry,
          children: new Map(),
        });
      } else {
        if (!current.children.has(part)) {
          current.children.set(part, { name: part, children: new Map() });
        }
        current = current.children.get(part)!;
      }
    }
  }
  return root;
}

export function formatTimeAgo(timestamp: string): string {
  const now = Date.now();
  const then = new Date(timestamp).getTime();
  const seconds = Math.floor((now - then) / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

/** Method → color mapping for sidebar badges */
export const METHOD_COLORS: Record<string, string> = {
  GET: "#4ec9b0",
  POST: "#dcdcaa",
  PUT: "#569cd6",
  PATCH: "#c586c0",
  DELETE: "#f44747",
};

export function statusColor(status: number): string {
  if (status < 300) return "#4ec9b0";
  if (status < 400) return "#dcdcaa";
  return "#f44747";
}

export function formatBody(body: string): string {
  try {
    return JSON.stringify(JSON.parse(body), null, 2);
  } catch {
    return body;
  }
}
