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

/** Filter tree to only include nodes matching the query (case-insensitive) */
export function filterTree(tree: TreeNode, query: string): TreeNode {
  if (!query.trim()) return tree;
  const lower = query.toLowerCase();

  function filterNode(node: TreeNode): TreeNode | null {
    // Leaf node — check if name matches
    if (node.entry) {
      return node.name.toLowerCase().includes(lower) ? node : null;
    }
    // Folder — keep if any children match
    const filtered = new Map<string, TreeNode>();
    for (const [key, child] of node.children) {
      const result = filterNode(child);
      if (result) filtered.set(key, result);
    }
    if (filtered.size === 0) return null;
    return { name: node.name, children: filtered };
  }

  const result = filterNode(tree);
  return result ?? { name: tree.name, children: new Map() };
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

/** Method → color mapping for sidebar badges. Semantic — never themed.
 *  Values reference the design tokens so tokens.css stays the single source. */
export const METHOD_COLORS: Record<string, string> = {
  GET: "var(--method-get)",
  POST: "var(--method-post)",
  PUT: "var(--method-put)",
  PATCH: "var(--method-patch)",
  DELETE: "var(--method-delete)",
};

/** Fallback for unknown methods (HEAD, OPTIONS, …). */
export const METHOD_COLOR_FALLBACK = "var(--text-mid)";

export function statusColor(status: number): string {
  if (status < 300) return "var(--ok)";
  if (status < 400) return "var(--warn)";
  return "var(--err)";
}

/** Human-readable byte size, e.g. 1234 → "1.2 KB". */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  return `${(kb / 1024).toFixed(1)} MB`;
}

export function formatBody(body: string): string {
  try {
    return JSON.stringify(JSON.parse(body), null, 2);
  } catch {
    return body;
  }
}
