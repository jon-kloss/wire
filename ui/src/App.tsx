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
import type { TreeNode } from "./utils";
import {
  buildTree,
  filterTree,
  formatTimeAgo,
  METHOD_COLORS,
  statusColor,
  formatBody,
} from "./utils";
import { PromptModal } from "./PromptModal";
import "./App.css";

const MonacoEditor = lazy(() => import("@monaco-editor/react"));

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

  // Sidebar state
  const [sidebarTab, setSidebarTab] = useState<"collections" | "activity">(
    "collections"
  );
  const [filterText, setFilterText] = useState("");

  // History state
  const [history, setHistory] = useState<HistoryEntry[]>([]);

  // Prompt modal state (replaces window.prompt which doesn't work in Tauri v2)
  const [promptState, setPromptState] = useState<{
    title: string;
    defaultValue: string;
    resolve: (value: string | null) => void;
  } | null>(null);

  const showPrompt = useCallback(
    (title: string, defaultValue = ""): Promise<string | null> => {
      return new Promise((resolve) => {
        setPromptState((prev) => {
          prev?.resolve(null); // cancel any in-flight prompt
          return { title, defaultValue, resolve };
        });
      });
    },
    []
  );

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

  const handleNewRequest = useCallback(() => {
    setMethod("GET");
    setUrl("");
    setHeadersText("");
    setBodyText("");
    setSelectedRequestPath(null);
    setResponse(null);
    setError(null);
  }, []);

  const handleOpenCollection = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected) return;

    try {
      // Look for .wire/ subdirectory or use the selected directory directly
      // open_collection expects the .wire dir itself
      const wireDir = selected as string;
      const info = await invoke<IpcCollectionInfo>("open_collection", {
        wireDir,
      });
      setCollection(info);
      setCollectionPath(wireDir);
      setSelectedEnv(info.active_env ?? null);
    } catch (err) {
      setError(String(err));
    }
  }, []);

  const handleNewCollection = useCallback(async () => {
    const name = await showPrompt("Collection name:");
    if (!name?.trim()) return;

    const selected = await open({
      directory: true,
      multiple: false,
      title: "Choose directory for new collection",
    });
    if (!selected) return;

    try {
      const info = await invoke<IpcCollectionInfo>("create_collection_cmd", {
        name: name.trim(),
        parentDir: selected as string,
      });
      setCollection(info);
      setCollectionPath((selected as string) + "/.wire");
      setSelectedEnv(info.active_env ?? null);
    } catch (err) {
      setError(String(err));
    }
  }, [showPrompt]);

  const handleImportFromUrl = useCallback(async () => {
    const importUrl = await showPrompt("Import from URL:", "https://");
    if (!importUrl?.trim()) return;

    setUrl(importUrl.trim());
    setMethod("GET");
    setSelectedRequestPath(null);
    setResponse(null);
    setError(null);
  }, [showPrompt]);

  const handleSaveRequest = useCallback(async () => {
    if (!collectionPath) {
      setError("Open or create a collection first to save requests.");
      return;
    }

    let defaultName = "request";
    try {
      if (url) defaultName = new URL(url).pathname.split("/").pop() || "request";
    } catch {
      // invalid URL — use default
    }
    const name = await showPrompt("Request name:", defaultName);
    if (!name?.trim()) return;

    const fileName = name.trim().replace(/\s+/g, "-").toLowerCase() + ".wire.yaml";
    const filePath = collectionPath + "/requests/" + fileName;

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
        name: name.trim(),
        method,
        url,
        headers,
        params: {},
        body,
      };

      await invoke("save_request", { path: filePath, request });

      // Refresh sidebar
      const info = await invoke<IpcCollectionInfo>("open_collection", {
        wireDir: collectionPath,
      });
      setCollection(info);
    } catch (err) {
      setError(String(err));
    }
  }, [method, url, headersText, bodyText, collectionPath, showPrompt]);

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

  const tree =
    collection && collectionPath
      ? buildTree(collection.requests, collectionPath)
      : null;

  const filteredTree = tree ? filterTree(tree, filterText) : null;

  const sortedChildren = filteredTree
    ? [...filteredTree.children.values()].sort((a, b) => {
        const aIsFolder = !a.entry;
        const bIsFolder = !b.entry;
        if (aIsFolder !== bIsFolder) return aIsFolder ? -1 : 1;
        return a.name.localeCompare(b.name);
      })
    : [];

  return (
    <div className="app">
      {/* Left Panel: Sidebar */}
      <aside className="sidebar">
        <button className="new-request-btn" onClick={handleNewRequest}>
          + New Request
        </button>

        <div className="sidebar-tabs">
          <button
            className={`sidebar-tab ${sidebarTab === "collections" ? "active" : ""}`}
            onClick={() => setSidebarTab("collections")}
          >
            Collections
          </button>
          <button
            className={`sidebar-tab ${sidebarTab === "activity" ? "active" : ""}`}
            onClick={() => setSidebarTab("activity")}
          >
            Activity
          </button>
        </div>

        {sidebarTab === "collections" && (
          <div className="sidebar-content">
            <div className="sidebar-controls">
              <input
                className="filter-input"
                type="text"
                placeholder="Filter collections..."
                value={filterText}
                onChange={(e) => setFilterText(e.target.value)}
              />

              {collection && collection.environments.length > 0 && (
                <select
                  className="env-select"
                  value={selectedEnv ?? ""}
                  onChange={(e) =>
                    setSelectedEnv(
                      e.target.value === "" ? null : e.target.value
                    )
                  }
                >
                  <option value="">(no env)</option>
                  {collection.environments.map((env) => (
                    <option key={env} value={env}>
                      {env}
                    </option>
                  ))}
                </select>
              )}

              <div className="sidebar-action-buttons">
                <button
                  className="sidebar-action-btn"
                  onClick={handleNewCollection}
                >
                  New Collection
                </button>
                <button
                  className="sidebar-action-btn"
                  onClick={handleOpenCollection}
                >
                  Import
                </button>
                <button
                  className="sidebar-action-btn"
                  onClick={handleImportFromUrl}
                >
                  Import from URL
                </button>
              </div>
            </div>

            <div className="sidebar-tree">
              {!collection && (
                <div className="empty-state">
                  <p className="empty-state-title">
                    No Collections Available
                  </p>
                  <p className="empty-state-hint">
                    You can create collections here or import existing ones.
                  </p>
                </div>
              )}
              {collection && sortedChildren.length === 0 && (
                <p className="placeholder">
                  {filterText ? "No matching requests" : "No requests found"}
                </p>
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
          </div>
        )}

        {sidebarTab === "activity" && (
          <div className="sidebar-content">
            <div className="history-list">
              {history.length === 0 && (
                <p className="placeholder">No activity yet</p>
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
          </div>
        )}
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
          <button className="save-btn" onClick={handleSaveRequest}>
            Save
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

      {promptState && (
        <PromptModal
          title={promptState.title}
          defaultValue={promptState.defaultValue}
          onConfirm={(value) => {
            promptState.resolve(value);
            setPromptState(null);
          }}
          onCancel={() => {
            promptState.resolve(null);
            setPromptState(null);
          }}
        />
      )}
    </div>
  );
}

export default App;
