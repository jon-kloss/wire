import { useState, useCallback, useEffect, useRef, lazy, Suspense } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  IpcResponse,
  IpcCollectionInfo,
  IpcRequestEntry,
  IpcScanResult,
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
  const urlInputRef = useRef<HTMLInputElement>(null);

  // Request builder state
  const [method, setMethod] = useState("GET");
  const [url, setUrl] = useState("");
  const [headersText, setHeadersText] = useState("");
  const [bodyText, setBodyText] = useState("");
  const [activeTab, setActiveTab] = useState<
    "query" | "headers" | "auth" | "body" | "tests" | "pre-run"
  >("query");

  // Response state
  const [response, setResponse] = useState<IpcResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [responseTab, setResponseTab] = useState<"body" | "headers">("body");

  // Collection state (supports multiple collections)
  const [collections, setCollections] = useState<
    Array<{ info: IpcCollectionInfo; path: string }>
  >([]);
  const [activeCollectionPath, setActiveCollectionPath] = useState<
    string | null
  >(null);
  const [expandedCollections, setExpandedCollections] = useState<Set<string>>(
    new Set()
  );
  // Per-collection environment state (keyed by collection path)
  const [envSelectedMap, setEnvSelectedMap] = useState<Record<string, string | null>>({});
  const [envVarsMap, setEnvVarsMap] = useState<Record<string, Record<string, string>>>({});
  const [selectedRequestPath, setSelectedRequestPath] = useState<string | null>(
    null
  );
  const [selectedRequestName, setSelectedRequestName] = useState<string | null>(
    null
  );

  // Sidebar state
  const [sidebarTab, setSidebarTab] = useState<"collections" | "activity">(
    "collections"
  );
  const [filterText, setFilterText] = useState("");
  const [dropdownOpen, setDropdownOpen] = useState(false);

  // Derived env state for the active collection
  const activeSelectedEnv = activeCollectionPath ? envSelectedMap[activeCollectionPath] ?? null : null;
  const activeEnvVars = activeCollectionPath ? envVarsMap[activeCollectionPath] ?? {} : {};

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

  // Load environment variables when active collection changes (sync from already-loaded map)
  // Per-collection env loading happens inline in the env select onChange and on collection open

  const envVarsMapRef = useRef(envVarsMap);
  envVarsMapRef.current = envVarsMap;

  const handleSaveEnvVar = useCallback(
    async (collectionPath: string, envName: string, key: string, value: string) => {
      const currentVars = envVarsMapRef.current[collectionPath] ?? {};
      const updated = { ...currentVars, [key]: value };
      setEnvVarsMap((prev) => ({ ...prev, [collectionPath]: updated }));
      try {
        await invoke("save_environment", {
          wireDir: collectionPath,
          envName,
          variables: updated,
        });
      } catch (err) {
        setError(String(err));
      }
    },
    []
  );

  const handleNewRequest = useCallback(() => {
    setMethod("GET");
    setUrl("");
    setHeadersText("");
    setBodyText("");
    setSelectedRequestPath(null);
    setSelectedRequestName(null);
    setResponse(null);
    setError(null);
  }, []);

  const handleOpenCollection = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected) return;

    try {
      // User may select the parent dir (e.g., "myproject") or the .wire dir directly.
      // Normalize to the .wire dir for the backend.
      const raw = selected as string;
      const wireDir = raw.endsWith("/.wire") ? raw : raw + "/.wire";
      const info = await invoke<IpcCollectionInfo>("open_collection", {
        wireDir,
      });
      setCollections((prev) => {
        const existing = prev.findIndex((c) => c.path === wireDir);
        if (existing >= 0) {
          const updated = [...prev];
          updated[existing] = { info, path: wireDir };
          return updated;
        }
        return [...prev, { info, path: wireDir }];
      });
      setActiveCollectionPath(wireDir);
      setExpandedCollections((prev) => new Set(prev).add(wireDir));
      const envName = info.active_env ?? null;
      setEnvSelectedMap((prev) => ({ ...prev, [wireDir]: envName }));
      if (envName) {
        invoke<Record<string, string>>("get_environment", { wireDir, envName })
          .then((vars) => setEnvVarsMap((prev) => ({ ...prev, [wireDir]: vars })))
          .catch(() => {});
      }
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
      const newPath = (selected as string) + "/.wire";
      setCollections((prev) => [...prev, { info, path: newPath }]);
      setActiveCollectionPath(newPath);
      setExpandedCollections((prev) => new Set(prev).add(newPath));
      setEnvSelectedMap((prev) => ({ ...prev, [newPath]: info.active_env ?? null }));
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
    setSelectedRequestName(null);
    setResponse(null);
    setError(null);
  }, [showPrompt]);

  const handleImportFromCodebase = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false, title: "Select project to scan" });
    if (!selected) return;

    try {
      const projectDir = selected as string;
      // Use the project directory itself as the output location
      const result = await invoke<IpcScanResult>("scan_codebase", {
        projectDir,
        outputDir: projectDir,
      });

      if (result.endpoints_found === 0) {
        setError(
          `No HTTP endpoints found in ${projectDir}. ` +
          `Scanned ${result.files_scanned} files (detected: ${result.framework}).`
        );
        return;
      }

      if (result.collection && result.wire_dir) {
        const wireDir = result.wire_dir;
        setCollections((prev) => {
          const existing = prev.findIndex((c) => c.path === wireDir);
          if (existing >= 0) {
            const updated = [...prev];
            updated[existing] = { info: result.collection!, path: wireDir };
            return updated;
          }
          return [...prev, { info: result.collection!, path: wireDir }];
        });
        setActiveCollectionPath(wireDir);
        setExpandedCollections((prev) => new Set(prev).add(wireDir));
        const envName = result.collection!.active_env ?? null;
        setEnvSelectedMap((prev) => ({ ...prev, [wireDir]: envName }));
        if (envName) {
          invoke<Record<string, string>>("get_environment", { wireDir, envName })
            .then((vars) => setEnvVarsMap((prev) => ({ ...prev, [wireDir]: vars })))
            .catch(() => {});
        }
      }
    } catch (err) {
      setError(String(err));
    }
  }, []);

  const handleDeleteCollection = useCallback((path: string) => {
    setCollections((prev) => prev.filter((c) => c.path !== path));
    setActiveCollectionPath((prev) => (prev === path ? null : prev));
    setSelectedRequestPath((prev) => {
      if (prev && prev.startsWith(path)) {
        setSelectedRequestName(null);
        return null;
      }
      return prev;
    });
    setExpandedCollections((prev) => {
      const next = new Set(prev);
      next.delete(path);
      return next;
    });
    setEnvSelectedMap((prev) => {
      const next = { ...prev };
      delete next[path];
      return next;
    });
    setEnvVarsMap((prev) => {
      const next = { ...prev };
      delete next[path];
      return next;
    });
  }, []);

  const handleRenameCollection = useCallback(
    async (path: string, currentName: string) => {
      const newName = await showPrompt("Rename collection:", currentName);
      if (!newName?.trim() || newName.trim() === currentName) return;

      try {
        const info = await invoke<IpcCollectionInfo>("rename_collection_cmd", {
          wireDir: path,
          newName: newName.trim(),
        });
        setCollections((prev) =>
          prev.map((c) => (c.path === path ? { info, path } : c))
        );
      } catch (err) {
        setError(String(err));
      }
    },
    [showPrompt]
  );

  const handleAddRequest = useCallback(
    async (collectionPath: string) => {
      const name = await showPrompt("New request name:", "");
      if (!name?.trim()) return;

      const fileName =
        name.trim().replace(/\s+/g, "-").toLowerCase() + ".wire.yaml";
      const filePath = collectionPath + "/requests/" + fileName;

      try {
        const request: WireRequest = {
          name: name.trim(),
          method: "GET",
          url: "",
          headers: {},
          params: {},
          body: null,
        };

        await invoke("save_request", { path: filePath, request });

        // Refresh the collection
        const info = await invoke<IpcCollectionInfo>("open_collection", {
          wireDir: collectionPath,
        });
        setCollections((prev) =>
          prev.map((c) => (c.path === collectionPath ? { info, path: c.path } : c))
        );

        // Set as active and expand
        setActiveCollectionPath(collectionPath);
        setExpandedCollections((prev) => new Set(prev).add(collectionPath));
        setEnvSelectedMap((prev) => ({ ...prev, [collectionPath]: info.active_env ?? null }));

        // Load the new request into builder with base URL template
        setMethod("GET");
        setUrl("{{schema}}://{{baseUrl}}/");
        setHeadersText("");
        setBodyText("");
        setSelectedRequestPath(filePath);
        setSelectedRequestName(name.trim());
        setResponse(null);
        setError(null);
      } catch (err) {
        setError(String(err));
      }
    },
    [showPrompt]
  );

  const handleSaveRequest = useCallback(async () => {
    if (!activeCollectionPath) {
      setError("Open or create a collection first to save requests.");
      return;
    }

    // Determine save path: reuse existing path or prompt for a new name
    let filePath: string;
    let requestName: string;

    if (
      selectedRequestPath &&
      selectedRequestPath.startsWith(activeCollectionPath)
    ) {
      // Existing request in the active collection — overwrite in place
      filePath = selectedRequestPath;
      requestName = selectedRequestName ?? "request";
    } else {
      // New request — prompt for name
      let defaultName = "request";
      try {
        if (url) defaultName = new URL(url).pathname.split("/").pop() || "request";
      } catch {
        // invalid URL — use default
      }
      const name = await showPrompt("Request name:", defaultName);
      if (!name?.trim()) return;

      requestName = name.trim();
      const fileName = requestName.replace(/\s+/g, "-").toLowerCase() + ".wire.yaml";
      filePath = activeCollectionPath + "/requests/" + fileName;
    }

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
        name: requestName,
        method,
        url,
        headers,
        params: {},
        body,
      };

      await invoke("save_request", { path: filePath, request });

      // Track the saved path/name so subsequent saves overwrite
      setSelectedRequestPath(filePath);
      setSelectedRequestName(requestName);

      // Refresh the specific collection
      const info = await invoke<IpcCollectionInfo>("open_collection", {
        wireDir: activeCollectionPath,
      });
      setCollections((prev) =>
        prev.map((c) =>
          c.path === activeCollectionPath ? { info, path: c.path } : c
        )
      );
    } catch (err) {
      setError(String(err));
    }
  }, [method, url, headersText, bodyText, activeCollectionPath, selectedRequestPath, selectedRequestName, showPrompt]);

  const handleSelectRequest = useCallback(
    async (entry: IpcRequestEntry) => {
      try {
        const req = await invoke<WireRequest>("read_request", {
          file: entry.path,
        });
        setMethod(req.method);

        // Prepend base URL template if URL is a relative path
        let loadedUrl = req.url;
        if (
          loadedUrl.startsWith("/") &&
          !loadedUrl.startsWith("{{schema}}")
        ) {
          loadedUrl = "{{schema}}://{{baseUrl}}" + loadedUrl;
        }
        setUrl(loadedUrl);

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
        setSelectedRequestName(req.name);
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
        env: activeSelectedEnv,
      });

      setResponse(result);
      refreshHistory();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const toggleCollection = useCallback((path: string) => {
    setExpandedCollections((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

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
            onClick={() => {
              setSidebarTab("collections");
              setDropdownOpen(false);
            }}
          >
            Collections
          </button>
          <button
            className={`sidebar-tab ${sidebarTab === "activity" ? "active" : ""}`}
            onClick={() => {
              setSidebarTab("activity");
              setDropdownOpen(false);
            }}
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

              <div className="dropdown-wrapper">
                <button
                  className="sidebar-action-btn dropdown-trigger"
                  onClick={() => setDropdownOpen(!dropdownOpen)}
                >
                  Collections &#x25BE;
                </button>
                {dropdownOpen && (
                  <>
                    <div
                      className="dropdown-backdrop"
                      onClick={() => setDropdownOpen(false)}
                    />
                    <div className="dropdown-menu">
                      <button
                        className="dropdown-item"
                        onClick={() => {
                          setDropdownOpen(false);
                          handleNewCollection();
                        }}
                      >
                        New Collection
                      </button>
                      <button
                        className="dropdown-item"
                        onClick={() => {
                          setDropdownOpen(false);
                          handleOpenCollection();
                        }}
                      >
                        Import Collection
                      </button>
                      <button
                        className="dropdown-item"
                        onClick={() => {
                          setDropdownOpen(false);
                          handleImportFromUrl();
                        }}
                      >
                        Import from URL
                      </button>
                      <button
                        className="dropdown-item"
                        onClick={() => {
                          setDropdownOpen(false);
                          handleImportFromCodebase();
                        }}
                      >
                        Generate from Codebase
                      </button>
                    </div>
                  </>
                )}
              </div>
            </div>

            <div className="sidebar-tree">
              {collections.length === 0 && (
                <div className="empty-state">
                  <p className="empty-state-title">
                    No Collections Available
                  </p>
                  <p className="empty-state-hint">
                    Use the Collections menu above to create or import one.
                  </p>
                </div>
              )}
              {collections.map(({ info, path }) => {
                const isExpanded = expandedCollections.has(path);
                const tree = buildTree(info.requests, path);
                const filtered = filterTree(tree, filterText);
                const sorted = [...filtered.children.values()].sort(
                  (a, b) => {
                    const aIsFolder = !a.entry;
                    const bIsFolder = !b.entry;
                    if (aIsFolder !== bIsFolder) return aIsFolder ? -1 : 1;
                    return a.name.localeCompare(b.name);
                  }
                );
                const isActive = path === activeCollectionPath;
                return (
                  <div
                    key={path}
                    className={`collection-accordion ${isActive ? "active" : ""}`}
                  >
                    <div
                      className="collection-header"
                      onClick={() => toggleCollection(path)}
                    >
                      <span className="folder-icon">
                        {isExpanded ? "\u25BE" : "\u25B8"}
                      </span>
                      <span className="collection-name">{info.name}</span>
                      <span className="collection-count">
                        {info.requests.length}
                      </span>
                      <div className="collection-actions">
                        <button
                          className="collection-action-btn"
                          title="Add request"
                          onClick={(e) => {
                            e.stopPropagation();
                            handleAddRequest(path);
                          }}
                        >
                          +
                        </button>
                        <button
                          className="collection-action-btn"
                          title="Rename collection"
                          onClick={(e) => {
                            e.stopPropagation();
                            handleRenameCollection(path, info.name);
                          }}
                        >
                          &#x270E;
                        </button>
                        <button
                          className="collection-action-btn collection-action-delete"
                          title="Remove collection"
                          onClick={(e) => {
                            e.stopPropagation();
                            handleDeleteCollection(path);
                          }}
                        >
                          &#x2715;
                        </button>
                      </div>
                    </div>
                    {isExpanded && (
                      <div className="collection-requests">
                        {sorted.length === 0 && (
                          <p className="placeholder collection-empty">
                            {filterText
                              ? "No matching requests"
                              : "No requests yet"}
                          </p>
                        )}
                        {sorted.map((child) => (
                          <TreeItem
                            key={child.entry?.path ?? child.name}
                            node={child}
                            depth={1}
                            onSelect={async (entry) => {
                              // Sync backend state when switching collections
                              if (path !== activeCollectionPath) {
                                try {
                                  await invoke("open_collection", { wireDir: path });
                                } catch { /* already open or will fail on send */ }
                              }
                              setActiveCollectionPath(path);
                              handleSelectRequest(entry);
                            }}
                            selectedPath={selectedRequestPath}
                          />
                        ))}
                        {info.environments.length > 0 && (
                          <div className="collection-env-section">
                            <div className="collection-env-header">
                              <select
                                className="env-select"
                                value={envSelectedMap[path] ?? ""}
                                onChange={(e) => {
                                  const val = e.target.value === "" ? null : e.target.value;
                                  setEnvSelectedMap((prev) => ({ ...prev, [path]: val }));
                                  // Load the env vars for this collection
                                  if (val) {
                                    invoke<Record<string, string>>("get_environment", {
                                      wireDir: path,
                                      envName: val,
                                    })
                                      .then((vars) => setEnvVarsMap((prev) => ({ ...prev, [path]: vars })))
                                      .catch(() => setEnvVarsMap((prev) => ({ ...prev, [path]: {} })));
                                  } else {
                                    setEnvVarsMap((prev) => ({ ...prev, [path]: {} }));
                                  }
                                }}
                              >
                                <option value="">(no env)</option>
                                {info.environments.map((env) => (
                                  <option key={env} value={env}>
                                    {env}
                                  </option>
                                ))}
                              </select>
                            </div>
                            {envSelectedMap[path] &&
                              envVarsMap[path] &&
                              Object.keys(envVarsMap[path]).length > 0 && (
                                <div className="env-vars-editor">
                                  {Object.entries(envVarsMap[path]).map(([key, value]) => (
                                    <div key={key} className="env-var-row">
                                      <label className="env-var-label">{key}</label>
                                      <input
                                        className="env-var-input"
                                        type="text"
                                        value={value}
                                        onChange={(e) =>
                                          handleSaveEnvVar(path, envSelectedMap[path]!, key, e.target.value)
                                        }
                                      />
                                    </div>
                                  ))}
                                </div>
                              )}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
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
                      setSelectedRequestName(null);
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
          <div className="url-input-wrapper">
            <div
              className="url-highlight"
              aria-hidden="true"
              onClick={() => urlInputRef.current?.focus()}
            >
              {url
                ? url.split(/(\{\{[^}]+\}\})/).map((part, i) => {
                    const varMatch = part.match(/^\{\{([^}]+)\}\}$/);
                    if (varMatch) {
                      const varName = varMatch[1];
                      const resolved = activeEnvVars[varName];
                      return (
                        <span
                          key={i}
                          className="url-variable"
                          data-tooltip={
                            resolved !== undefined
                              ? resolved
                              : "(not set)"
                          }
                        >
                          {part}
                        </span>
                      );
                    }
                    return <span key={i}>{part}</span>;
                  })
                : <span className="url-placeholder">Enter request URL...</span>}
            </div>
            <input
              ref={urlInputRef}
              className="url-input"
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleSend()}
            />
          </div>
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
              className={`tab ${activeTab === "query" ? "active" : ""}`}
              onClick={() => setActiveTab("query")}
            >
              Query
            </button>
            <button
              className={`tab ${activeTab === "headers" ? "active" : ""}`}
              onClick={() => setActiveTab("headers")}
            >
              Headers
            </button>
            <button
              className={`tab ${activeTab === "auth" ? "active" : ""}`}
              onClick={() => setActiveTab("auth")}
            >
              Auth
            </button>
            <button
              className={`tab ${activeTab === "body" ? "active" : ""}`}
              onClick={() => setActiveTab("body")}
            >
              Body
            </button>
            <button
              className={`tab ${activeTab === "tests" ? "active" : ""}`}
              onClick={() => setActiveTab("tests")}
            >
              Tests
            </button>
            <button
              className={`tab ${activeTab === "pre-run" ? "active" : ""}`}
              onClick={() => setActiveTab("pre-run")}
            >
              Pre Run
            </button>
          </div>
          <div className="tab-content">
            {activeTab === "query" && (
              <div className="tab-placeholder">
                <p className="placeholder">Query Parameters</p>
              </div>
            )}
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
            {activeTab === "auth" && (
              <div className="tab-placeholder">
                <p className="placeholder">Authentication</p>
              </div>
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
            {activeTab === "tests" && (
              <div className="tab-placeholder">
                <p className="placeholder">Tests</p>
              </div>
            )}
            {activeTab === "pre-run" && (
              <div className="tab-placeholder">
                <p className="placeholder">Pre-request Script</p>
              </div>
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
