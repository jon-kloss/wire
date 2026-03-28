import { useState, useCallback, useEffect, lazy, Suspense } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  IpcResponse,
  IpcCollectionInfo,
  IpcRequestEntry,
  HistoryEntry,
  WireRequest,
  WireBody,
} from "./types";
import "./App.css";

const MonacoEditor = lazy(() => import("@monaco-editor/react"));

function formatTimeAgo(timestamp: string): string {
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
const METHOD_COLORS: Record<string, string> = {
  GET: "#4ec9b0",
  POST: "#dcdcaa",
  PUT: "#569cd6",
  PATCH: "#c586c0",
  DELETE: "#f44747",
};

/** Group flat request list into a folder tree */
interface TreeNode {
  name: string;
  /** Leaf nodes have a request entry */
  entry?: IpcRequestEntry;
  children: Map<string, TreeNode>;
}

function buildTree(requests: IpcRequestEntry[], basePath: string): TreeNode {
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

function TreeItem({
  node,
  depth,
  onSelect,
  selectedPath,
}: {
  node: TreeNode;
  depth: number;
  onSelect: (entry: IpcRequestEntry) => void;
  selectedPath: string | null;
}) {
  const [expanded, setExpanded] = useState(true);
  const isFolder = !node.entry && node.children.size > 0;
  const isSelected = node.entry?.path === selectedPath;

  if (node.entry) {
    const color = METHOD_COLORS[node.entry.method] ?? "#d4d4d4";
    return (
      <div
        className={`tree-item tree-request ${isSelected ? "selected" : ""}`}
        style={{ paddingLeft: depth * 16 + 8 }}
        onClick={() => onSelect(node.entry!)}
      >
        <span className="method-badge" style={{ color }}>
          {node.entry.method}
        </span>
        <span className="request-name">{node.name}</span>
      </div>
    );
  }

  if (isFolder) {
    const sorted = [...node.children.values()].sort((a, b) => {
      // Folders first, then requests
      const aIsFolder = !a.entry;
      const bIsFolder = !b.entry;
      if (aIsFolder !== bIsFolder) return aIsFolder ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    return (
      <div>
        <div
          className="tree-item tree-folder"
          style={{ paddingLeft: depth * 16 + 8 }}
          onClick={() => setExpanded(!expanded)}
        >
          <span className="folder-icon">{expanded ? "\u25BE" : "\u25B8"}</span>
          <span className="folder-name">{node.name}</span>
        </div>
        {expanded &&
          sorted.map((child) => (
            <TreeItem
              key={child.entry?.path ?? child.name}
              node={child}
              depth={depth + 1}
              onSelect={onSelect}
              selectedPath={selectedPath}
            />
          ))}
      </div>
    );
  }

  return null;
}

function App() {
  // Request builder state
  const [method, setMethod] = useState("GET");
  const [url, setUrl] = useState("");
  const [headersText, setHeadersText] = useState("");
  const [bodyText, setBodyText] = useState("");
  const [activeTab, setActiveTab] = useState<"headers" | "body">("headers");

  // Response state
  const [response, setResponse] = useState<IpcResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [responseTab, setResponseTab] = useState<"body" | "headers">("body");

  // Collection state
  const [collection, setCollection] = useState<IpcCollectionInfo | null>(null);
  const [collectionPath, setCollectionPath] = useState<string | null>(null);
  const [selectedEnv, setSelectedEnv] = useState<string | null>(null);
  const [selectedRequestPath, setSelectedRequestPath] = useState<string | null>(
    null
  );

  // History state
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [historyExpanded, setHistoryExpanded] = useState(true);

  const refreshHistory = useCallback(async () => {
    try {
      const entries = await invoke<HistoryEntry[]>("list_history", {
        limit: 100,
      });
      setHistory(entries.reverse()); // most recent first
    } catch {
      // History is non-critical — silently ignore errors
    }
  }, []);

  // Load history on mount
  useEffect(() => {
    refreshHistory();
  }, [refreshHistory]);

  const handleOpenCollection = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected) return;

    try {
      // Look for .wire/ subdirectory or use the selected directory directly
      const info = await invoke<IpcCollectionInfo>("open_collection", {
        wireDir: selected,
      });
      setCollection(info);
      setCollectionPath(selected as string);
      setSelectedEnv(info.active_env ?? null);
    } catch (err) {
      setError(String(err));
    }
  }, []);

  const handleSelectRequest = useCallback(
    async (entry: IpcRequestEntry) => {
      try {
        const req = await invoke<WireRequest>("read_request", {
          file: entry.path,
        });
        setMethod(req.method);
        setUrl(req.url);

        // Build headers text
        const headerLines = Object.entries(req.headers)
          .map(([k, v]) => `${k}: ${v}`)
          .join("\n");
        setHeadersText(headerLines);

        // Build body text
        if (req.body) {
          if (req.body.type === "json") {
            setBodyText(JSON.stringify(req.body.content, null, 2));
          } else {
            setBodyText(String(req.body.content));
          }
        } else {
          setBodyText("");
        }

        setSelectedRequestPath(entry.path);
        setResponse(null);
        setError(null);
      } catch (err) {
        setError(String(err));
      }
    },
    []
  );

  const handleSend = async () => {
    if (!url.trim()) return;

    setLoading(true);
    setError(null);
    setResponse(null);

    try {
      const headers: Record<string, string> = {};
      if (headersText.trim()) {
        for (const line of headersText.split("\n")) {
          const idx = line.indexOf(":");
          if (idx > 0) {
            headers[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
          }
        }
      }

      let body: WireBody | null = null;
      if (bodyText.trim() && ["POST", "PUT", "PATCH"].includes(method)) {
        try {
          body = { type: "json", content: JSON.parse(bodyText) };
        } catch {
          body = { type: "text", content: bodyText };
        }
      }

      const request: WireRequest = {
        name: "Quick Request",
        method,
        url,
        headers,
        params: {},
        body,
      };

      const result = await invoke<IpcResponse>("send_raw_request", {
        request,
        env: selectedEnv,
      });

      setResponse(result);
      refreshHistory();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const formatBody = (body: string): string => {
    try {
      return JSON.stringify(JSON.parse(body), null, 2);
    } catch {
      return body;
    }
  };

  const statusColor = (status: number): string => {
    if (status < 300) return "#4ec9b0";
    if (status < 400) return "#dcdcaa";
    return "#f44747";
  };

  const tree =
    collection && collectionPath
      ? buildTree(collection.requests, collectionPath)
      : null;

  const sortedChildren = tree
    ? [...tree.children.values()].sort((a, b) => {
        const aIsFolder = !a.entry;
        const bIsFolder = !b.entry;
        if (aIsFolder !== bIsFolder) return aIsFolder ? -1 : 1;
        return a.name.localeCompare(b.name);
      })
    : [];

  return (
    <div className="app">
      {/* Left Panel: Collection Tree */}
      <aside className="sidebar">
        <div className="panel-header">
          <h2>Collections</h2>
          <button className="open-btn" onClick={handleOpenCollection}>
            Open
          </button>
        </div>

        {collection && collection.environments.length > 0 && (
          <div className="env-switcher">
            <select
              className="env-select"
              value={selectedEnv ?? ""}
              onChange={(e) =>
                setSelectedEnv(e.target.value === "" ? null : e.target.value)
              }
            >
              <option value="">(no env)</option>
              {collection.environments.map((env) => (
                <option key={env} value={env}>
                  {env}
                </option>
              ))}
            </select>
          </div>
        )}

        <div className="panel-body">
          {!collection && (
            <p className="placeholder">No collections loaded</p>
          )}
          {collection && sortedChildren.length === 0 && (
            <p className="placeholder">No requests found</p>
          )}
          {sortedChildren.map((child) => (
            <TreeItem
              key={child.entry?.path ?? child.name}
              node={child}
              depth={0}
              onSelect={handleSelectRequest}
              selectedPath={selectedRequestPath}
            />
          ))}
        </div>

        {/* History Section */}
        <div className="history-section">
          <div
            className="history-header"
            onClick={() => setHistoryExpanded(!historyExpanded)}
          >
            <span className="folder-icon">
              {historyExpanded ? "\u25BE" : "\u25B8"}
            </span>
            <h2>History</h2>
            <span className="history-count">{history.length}</span>
          </div>
          {historyExpanded && (
            <div className="history-list">
              {history.length === 0 && (
                <p className="placeholder">No history yet</p>
              )}
              {history.map((entry, i) => {
                const color = METHOD_COLORS[entry.method] ?? "#d4d4d4";
                const truncatedUrl =
                  entry.url.length > 40
                    ? entry.url.slice(0, 40) + "\u2026"
                    : entry.url;
                const ago = formatTimeAgo(entry.timestamp);
                return (
                  <div
                    key={`${entry.timestamp}-${i}`}
                    className="tree-item history-entry"
                    onClick={() => {
                      setMethod(entry.method);
                      setUrl(entry.url);
                      setSelectedRequestPath(null);
                      setResponse(null);
                      setError(null);
                    }}
                  >
                    <span className="method-badge" style={{ color }}>
                      {entry.method}
                    </span>
                    <span className="history-url" title={entry.url}>
                      {truncatedUrl}
                    </span>
                    <span
                      className="history-status"
                      style={{ color: statusColor(entry.status) }}
                    >
                      {entry.status}
                    </span>
                    <span className="history-time">{ago}</span>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </aside>

      {/* Center Panel: Request Builder */}
      <main className="request-builder">
        <div className="url-bar">
          <select
            className="method-select"
            value={method}
            onChange={(e) => setMethod(e.target.value)}
          >
            <option value="GET">GET</option>
            <option value="POST">POST</option>
            <option value="PUT">PUT</option>
            <option value="PATCH">PATCH</option>
            <option value="DELETE">DELETE</option>
          </select>
          <input
            className="url-input"
            type="text"
            placeholder="Enter request URL..."
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSend()}
          />
          <button className="send-btn" onClick={handleSend} disabled={loading}>
            {loading ? "Sending..." : "Send"}
          </button>
        </div>
        <div className="request-tabs">
          <div className="tabs">
            <button
              className={`tab ${activeTab === "headers" ? "active" : ""}`}
              onClick={() => setActiveTab("headers")}
            >
              Headers
            </button>
            <button
              className={`tab ${activeTab === "body" ? "active" : ""}`}
              onClick={() => setActiveTab("body")}
            >
              Body
            </button>
          </div>
          <div className="tab-content">
            {activeTab === "headers" && (
              <textarea
                className="editor-area"
                placeholder={
                  "Content-Type: application/json\nAuthorization: Bearer token"
                }
                value={headersText}
                onChange={(e) => setHeadersText(e.target.value)}
              />
            )}
            {activeTab === "body" && (
              <Suspense
                fallback={
                  <textarea
                    className="editor-area"
                    placeholder={'{\n  "key": "value"\n}'}
                    value={bodyText}
                    onChange={(e) => setBodyText(e.target.value)}
                  />
                }
              >
                <MonacoEditor
                  height="100%"
                  defaultLanguage="json"
                  theme="vs-dark"
                  value={bodyText}
                  onChange={(value) => setBodyText(value ?? "")}
                  options={{
                    minimap: { enabled: false },
                    lineNumbers: "on",
                    wordWrap: "on",
                    scrollBeyondLastLine: false,
                    fontSize: 13,
                    fontFamily:
                      '"Cascadia Code", "Fira Code", "Consolas", monospace',
                    automaticLayout: true,
                  }}
                />
              </Suspense>
            )}
          </div>
        </div>
      </main>

      {/* Right Panel: Response Viewer */}
      <section className="response-viewer">
        <div className="panel-header">
          <h2>Response</h2>
          {response && (
            <div className="response-meta">
              <span
                className="status-badge"
                style={{ color: statusColor(response.status) }}
              >
                {response.status} {response.status_text}
              </span>
              <span className="meta-item">{response.elapsed_ms}ms</span>
              <span className="meta-item">{response.size_bytes}B</span>
            </div>
          )}
        </div>

        {error && (
          <div className="panel-body">
            <div className="error-message">{error}</div>
          </div>
        )}

        {response && (
          <>
            <div className="tabs">
              <button
                className={`tab ${responseTab === "body" ? "active" : ""}`}
                onClick={() => setResponseTab("body")}
              >
                Body
              </button>
              <button
                className={`tab ${responseTab === "headers" ? "active" : ""}`}
                onClick={() => setResponseTab("headers")}
              >
                Headers
              </button>
            </div>
            <div className="panel-body">
              {responseTab === "body" && (
                <pre className="response-body">
                  {formatBody(response.body)}
                </pre>
              )}
              {responseTab === "headers" && (
                <div className="response-headers">
                  {Object.entries(response.headers).map(([key, value]) => (
                    <div key={key} className="header-row">
                      <span className="header-key">{key}</span>
                      <span className="header-value">{value}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </>
        )}

        {!response && !error && (
          <div className="panel-body">
            <p className="placeholder">Send a request to see the response</p>
          </div>
        )}
      </section>
    </div>
  );
}

export default App;
