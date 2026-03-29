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
  Assertion,
  TestResult,
  DriftReport,
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

/** Recursively extract dotpaths from a JSON value */
function extractJsonPaths(
  value: unknown,
  prefix: string,
  out: Array<{ path: string; value: string }>,
  maxDepth: number
) {
  if (maxDepth <= 0) return;

  if (value === null || value === undefined) {
    out.push({ path: prefix, value: "null" });
    return;
  }

  if (Array.isArray(value)) {
    out.push({ path: prefix, value: `[${value.length} items]` });
    for (let i = 0; i < Math.min(value.length, 5); i++) {
      extractJsonPaths(value[i], `${prefix}[${i}]`, out, maxDepth - 1);
    }
    return;
  }

  if (typeof value === "object") {
    for (const [key, val] of Object.entries(value as Record<string, unknown>)) {
      extractJsonPaths(val, `${prefix}.${key}`, out, maxDepth - 1);
    }
    return;
  }

  // Primitive
  const display = typeof value === "string" ? value : String(value);
  out.push({ path: prefix, value: display.length > 60 ? display.slice(0, 57) + "..." : display });
}

function TreeItem({
  node,
  depth,
  onSelect,
  selectedPath,
  defaultExpanded = true,
}: {
  node: TreeNode;
  depth: number;
  onSelect: (entry: IpcRequestEntry) => void;
  selectedPath: string | null;
  defaultExpanded?: boolean;
}) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  useEffect(() => { setExpanded(defaultExpanded); }, [defaultExpanded]);
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
              defaultExpanded={defaultExpanded}
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
  const [queryParams, setQueryParams] = useState<Array<{ key: string; value: string; enabled?: boolean }>>([]);
  const [activeTab, setActiveTab] = useState<
    "query" | "headers" | "auth" | "body" | "tests" | "pre-run"
  >("query");

  // Response state
  const [response, setResponse] = useState<IpcResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [testResults, setTestResults] = useState<TestResult[]>([]);
  const [currentAssertions, setCurrentAssertions] = useState<Assertion[]>([]);
  const [responseSchema, setResponseSchema] = useState<[string, string][]>([]);
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
  const [expandedEnvSections, setExpandedEnvSections] = useState<Set<string>>(new Set());
  const [expandedTemplateSections, setExpandedTemplateSections] = useState<Set<string>>(new Set());
  const [foldersExpanded, setFoldersExpanded] = useState(true);
  const [selectedRequestPath, setSelectedRequestPath] = useState<string | null>(
    null
  );
  const [selectedRequestName, setSelectedRequestName] = useState<string | null>(
    null
  );
  const [extendsTemplate, setExtendsTemplate] = useState<string | null>(null);
  const [extendsTooltip, setExtendsTooltip] = useState<string>("");
  const [isEditingTemplate, setIsEditingTemplate] = useState(false);

  // Sidebar state
  const [sidebarTab, setSidebarTab] = useState<"collections" | "activity" | "drift">(
    "collections"
  );
  const [filterText, setFilterText] = useState("");
  const [dropdownOpen, setDropdownOpen] = useState(false);

  // Derived env state for the active collection
  const activeSelectedEnv = activeCollectionPath ? envSelectedMap[activeCollectionPath] ?? null : null;
  const activeEnvVars = activeCollectionPath ? envVarsMap[activeCollectionPath] ?? {} : {};

  // Derived template state for active collection
  const activeCollection = collections.find((c) => c.path === activeCollectionPath);
  const activeTemplates = activeCollection?.info.templates ?? [];

  // Drift state
  const [driftReport, setDriftReport] = useState<DriftReport | null>(null);
  const [driftLoading, setDriftLoading] = useState(false);
  const [driftProjectDir, setDriftProjectDir] = useState("");

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
    setQueryParams([]);
    setSelectedRequestPath(null);
    setSelectedRequestName(null);
    setIsEditingTemplate(false);
    setExtendsTemplate(null);
    setExtendsTooltip("");
    setResponse(null);
    setError(null);
    setTestResults([]);
    setCurrentAssertions([]);
    setResponseSchema([]);
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
    setIsEditingTemplate(false);
    setExtendsTemplate(null);
    setExtendsTooltip("");
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

      const params: Record<string, string> = {};
      for (const p of queryParams) {
        if (p.key.trim() && p.enabled !== false) {
          params[p.key.trim()] = p.value;
        }
      }

      const request: WireRequest = {
        name: requestName,
        method,
        url,
        headers,
        params,
        body,
        extends: extendsTemplate ?? undefined,
        tests: currentAssertions.length > 0 ? currentAssertions : undefined,
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
  }, [method, url, headersText, bodyText, queryParams, currentAssertions, activeCollectionPath, selectedRequestPath, selectedRequestName, extendsTemplate, showPrompt]);

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

        // Load query params
        if (req.params && Object.keys(req.params).length > 0) {
          setQueryParams(
            Object.entries(req.params).map(([key, value]) => ({ key, value, enabled: true }))
          );
        } else {
          setQueryParams([]);
        }

        setSelectedRequestPath(entry.path);
        setSelectedRequestName(req.name);
        setIsEditingTemplate(false);
        setExtendsTemplate(req.extends ?? null);

        // Build tooltip showing what's inherited from the template
        if (req.extends) {
          try {
            const tmpl = await invoke<WireRequest>("read_template", { name: req.extends });
            const parts: string[] = [];
            const headerNames = Object.keys(tmpl.headers);
            if (headerNames.length > 0) parts.push(...headerNames);
            const paramNames = Object.keys(tmpl.params);
            if (paramNames.length > 0) parts.push(...paramNames.map((p) => `${p} param`));
            if (tmpl.body) parts.push("body");
            setExtendsTooltip(parts.length > 0 ? `Inheriting ${parts.join(", ")}` : "");
          } catch {
            setExtendsTooltip("");
          }
        } else {
          setExtendsTooltip("");
        }

        setResponse(null);
        setError(null);
        setTestResults([]);
        setCurrentAssertions(req.tests ?? []);
        setResponseSchema(req.response_schema ?? []);
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

      const params: Record<string, string> = {};
      for (const p of queryParams) {
        if (p.key.trim() && p.enabled !== false) {
          params[p.key.trim()] = p.value;
        }
      }

      const request: WireRequest = {
        name: "Quick Request",
        method,
        url,
        headers,
        params,
        body,
      };

      const result = await invoke<IpcResponse>("send_raw_request", {
        request,
        env: activeSelectedEnv,
      });

      setResponse(result);
      refreshHistory();

      // Evaluate test assertions if any exist
      if (currentAssertions.length > 0) {
        try {
          const results = await invoke<TestResult[]>("evaluate_tests", {
            assertions: currentAssertions,
            response: result,
          });
          setTestResults(results);
        } catch {
          // Test evaluation is non-critical
        }
      }
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

  // Extract dotpaths from a response for the assertion field picker
  const [responseFieldsOpen, setResponseFieldsOpen] = useState(false);

  const extractResponseFields = useCallback((): Array<{ path: string; value: string }> => {
    if (!response) return [];
    const fields: Array<{ path: string; value: string }> = [];

    // Status
    fields.push({ path: "status", value: String(response.status) });

    // Elapsed
    fields.push({ path: "elapsed_ms", value: String(response.elapsed_ms) });

    // Headers
    for (const [key, value] of Object.entries(response.headers)) {
      fields.push({ path: `header.${key}`, value });
    }

    // Body fields
    try {
      const body = JSON.parse(response.body);
      extractJsonPaths(body, "body", fields, 3);
    } catch {
      // non-JSON body
    }

    return fields;
  }, [response]);

  // Helpers for assertion editor
  const getAssertionOperator = (a: Assertion): string => {
    if (a.equals !== undefined) return "equals";
    if (a.not_equals !== undefined) return "not_equals";
    if (a.contains !== undefined) return "contains";
    if (a.starts_with !== undefined) return "starts_with";
    if (a.ends_with !== undefined) return "ends_with";
    if (a.less_than !== undefined) return "less_than";
    if (a.greater_than !== undefined) return "greater_than";
    if (a.is_array !== undefined) return "is_array";
    if (a.is_object !== undefined) return "is_object";
    if (a.is_string !== undefined) return "is_string";
    if (a.is_number !== undefined) return "is_number";
    if (a.exists !== undefined) return "exists";
    if (a.body_contains !== undefined) return "body_contains";
    if (a.body_matches !== undefined) return "body_matches";
    return "equals";
  };

  const getAssertionValue = (a: Assertion): string => {
    const v =
      a.equals ?? a.not_equals ?? a.contains ?? a.starts_with ?? a.ends_with ??
      a.less_than ?? a.greater_than ?? a.is_array ?? a.is_object ??
      a.is_string ?? a.is_number ?? a.exists ?? a.body_contains ?? a.body_matches;
    if (v === undefined || v === null) return "";
    return String(v);
  };

  const buildAssertion = (field: string, operator: string, value: string): Assertion => {
    const a: Assertion = { field };
    const boolOps = ["is_array", "is_object", "is_string", "is_number", "exists"];
    const numOps = ["less_than", "greater_than"];

    if (boolOps.includes(operator)) {
      (a as Record<string, unknown>)[operator] = value !== "false";
    } else if (numOps.includes(operator)) {
      (a as Record<string, unknown>)[operator] = parseFloat(value) || 0;
    } else if (operator === "equals" || operator === "not_equals") {
      const num = Number(value);
      (a as Record<string, unknown>)[operator] = !isNaN(num) && value.trim() !== "" ? num : value;
    } else {
      (a as Record<string, unknown>)[operator] = value;
    }
    return a;
  };

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
          <button
            className={`sidebar-tab ${sidebarTab === "drift" ? "active" : ""}`}
            onClick={() => {
              setSidebarTab("drift");
              setDropdownOpen(false);
            }}
          >
            Drift
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
                      <div className="collection-body">
                        {info.environments.length > 0 && (
                          <div className="collection-env-accordion">
                            <div
                              className="collection-env-toggle"
                              onClick={() =>
                                setExpandedEnvSections((prev) => {
                                  const next = new Set(prev);
                                  if (next.has(path)) {
                                    next.delete(path);
                                  } else {
                                    next.add(path);
                                  }
                                  return next;
                                })
                              }
                            >
                              <span className="folder-icon">
                                {expandedEnvSections.has(path) ? "\u25BE" : "\u25B8"}
                              </span>
                              <span className="collection-env-label">Environments</span>
                              <span className="collection-count">
                                {info.environments.length}
                              </span>
                            </div>
                            {expandedEnvSections.has(path) && (
                              <div className="collection-env-section">
                                <select
                                  className="env-select"
                                  value={envSelectedMap[path] ?? ""}
                                  onChange={(e) => {
                                    const val = e.target.value === "" ? null : e.target.value;
                                    setEnvSelectedMap((prev) => ({ ...prev, [path]: val }));
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
                        <div className="collection-env-accordion">
                            <div
                              className="collection-env-toggle"
                              onClick={() =>
                                setExpandedTemplateSections((prev) => {
                                  const next = new Set(prev);
                                  if (next.has(path)) {
                                    next.delete(path);
                                  } else {
                                    next.add(path);
                                  }
                                  return next;
                                })
                              }
                            >
                              <span className="folder-icon">
                                {expandedTemplateSections.has(path) ? "\u25BE" : "\u25B8"}
                              </span>
                              <span className="collection-env-label">Templates</span>
                              <span className="collection-count">
                                {info.templates.length}
                              </span>
                              <button
                                className="template-add-btn"
                                title="New Template"
                                onClick={async (e) => {
                                  e.stopPropagation();
                                  const name = await showPrompt("Template name:", "");
                                  if (!name?.trim()) return;
                                  const tmplName = name.trim().replace(/\s+/g, "-").toLowerCase();
                                  try {
                                    const tmpl: WireRequest = {
                                      name: tmplName,
                                      method: "",
                                      url: "",
                                      headers: {},
                                      params: {},
                                      body: null,
                                    };
                                    await invoke("save_template", { name: tmplName, request: tmpl });
                                    const updated = await invoke<IpcCollectionInfo>("open_collection", { wireDir: path });
                                    setCollections((prev) =>
                                      prev.map((c) => c.path === path ? { info: updated, path: c.path } : c)
                                    );
                                    // Open the new template in the editor
                                    setActiveCollectionPath(path);
                                    setSelectedRequestPath(`${path}/templates/${tmplName}.wire.yaml`);
                                    setSelectedRequestName(tmplName);
                                    setIsEditingTemplate(true);
                                    setMethod("");
                                    setUrl("");
                                    setHeadersText("");
                                    setBodyText("");
                                    setQueryParams([]);
                                    setExtendsTemplate(null);
                                    setExtendsTooltip("");
                                    setResponse(null);
                                    setError(null);
                                    // Expand the templates section
                                    setExpandedTemplateSections((prev) => new Set([...prev, path]));
                                  } catch (err) {
                                    setError(String(err));
                                  }
                                }}
                              >
                                +
                              </button>
                            </div>
                            {expandedTemplateSections.has(path) && (
                              <div className="collection-env-section">
                                {info.templates.map((tmpl) => (
                                  <div
                                    key={tmpl}
                                    className={`template-sidebar-item ${
                                      selectedRequestPath === `${path}/templates/${tmpl}.wire.yaml`
                                        ? "active"
                                        : ""
                                    }`}
                                    onClick={async () => {
                                      if (path !== activeCollectionPath) {
                                        try {
                                          await invoke("open_collection", { wireDir: path });
                                        } catch { /* ignore */ }
                                      }
                                      setActiveCollectionPath(path);
                                      const tmplPath = `${path}/templates/${tmpl}.wire.yaml`;
                                      try {
                                        const req = await invoke<WireRequest>("read_template", { name: tmpl });
                                        setMethod(req.method || "GET");
                                        setUrl(req.url || "");
                                        const headerLines = Object.entries(req.headers)
                                          .map(([k, v]) => `${k}: ${v}`)
                                          .join("\n");
                                        setHeadersText(headerLines);
                                        if (req.body) {
                                          setBodyText(
                                            req.body.type === "json"
                                              ? JSON.stringify(req.body.content, null, 2)
                                              : String(req.body.content)
                                          );
                                        } else {
                                          setBodyText("");
                                        }
                                        setQueryParams(
                                          req.params && Object.keys(req.params).length > 0
                                            ? Object.entries(req.params).map(([key, value]) => ({
                                                key,
                                                value,
                                                enabled: true,
                                              }))
                                            : []
                                        );
                                        setSelectedRequestPath(tmplPath);
                                        setSelectedRequestName(tmpl);
                                        setIsEditingTemplate(true);
                                        setExtendsTemplate(null);
                                        setExtendsTooltip("");
                                        setResponse(null);
                                        setError(null);
                                        setTestResults([]);
                                        setCurrentAssertions(req.tests ?? []);
                                        setResponseSchema(req.response_schema ?? []);
                                      } catch (err) {
                                        setError(String(err));
                                      }
                                    }}
                                  >
                                    <span className="template-sidebar-icon">T</span>
                                    <span className="template-sidebar-name">{tmpl}</span>
                                    <button
                                      className={`template-default-star ${info.default_templates.includes(tmpl) ? "active" : ""}`}
                                      title={info.default_templates.includes(tmpl) ? "Remove from defaults" : "Set as default"}
                                      onClick={async (e) => {
                                        e.stopPropagation();
                                        try {
                                          const updated = await invoke<string[]>("toggle_default_template", {
                                            wireDir: path,
                                            template: tmpl,
                                          });
                                          setCollections((prev) =>
                                            prev.map((c) =>
                                              c.path === path
                                                ? { info: { ...c.info, default_templates: updated }, path: c.path }
                                                : c
                                            )
                                          );
                                        } catch (err) {
                                          setError(String(err));
                                        }
                                      }}
                                    >
                                      {info.default_templates.includes(tmpl) ? "\u2605" : "\u2606"}
                                    </button>
                                    <button
                                      className="template-delete-btn"
                                      title="Delete template"
                                      onClick={async (e) => {
                                        e.stopPropagation();
                                        try {
                                          await invoke("delete_template", { name: tmpl });
                                          const updated = await invoke<IpcCollectionInfo>("open_collection", { wireDir: path });
                                          setCollections((prev) =>
                                            prev.map((c) =>
                                              c.path === path ? { info: updated, path: c.path } : c
                                            )
                                          );
                                          // Clear editor if this template was selected
                                          if (selectedRequestPath === `${path}/templates/${tmpl}.wire.yaml`) {
                                            setSelectedRequestPath(null);
                                            setSelectedRequestName(null);
                                            setIsEditingTemplate(false);
                                            setHeadersText("");
                                            setBodyText("");
                                            setQueryParams([]);
                                          }
                                        } catch (err) {
                                          setError(String(err));
                                        }
                                      }}
                                    >
                                      &times;
                                    </button>
                                  </div>
                                ))}
                              </div>
                            )}
                          </div>
                        <div className="collection-requests">
                          {sorted.some((c) => !c.entry) && (
                            <button
                              className="collapse-all-btn"
                              onClick={() => setFoldersExpanded((prev) => !prev)}
                              title={foldersExpanded ? "Collapse all folders" : "Expand all folders"}
                            >
                              {foldersExpanded ? "\u25BC" : "\u25B6"}
                            </button>
                          )}
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
                                if (path !== activeCollectionPath) {
                                  try {
                                    await invoke("open_collection", { wireDir: path });
                                  } catch { /* already open or will fail on send */ }
                                }
                                setActiveCollectionPath(path);
                                handleSelectRequest(entry);
                              }}
                              selectedPath={selectedRequestPath}
                              defaultExpanded={foldersExpanded}
                            />
                          ))}
                        </div>
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
                      setIsEditingTemplate(false);
                      setExtendsTemplate(null);
                      setExtendsTooltip("");
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
        {sidebarTab === "drift" && (
          <div className="sidebar-content drift-panel">
            <div className="drift-controls">
              <select
                className="drift-collection-select"
                value={driftProjectDir}
                onChange={(e) => {
                  setDriftProjectDir(e.target.value);
                  setDriftReport(null);
                  // Switch to this collection
                  const col = collections.find((c) => c.info.source_dir === e.target.value);
                  if (col) {
                    setActiveCollectionPath(col.path);
                    invoke("open_collection", { wireDir: col.path }).catch(() => {});
                  }
                }}
              >
                <option value="">Select collection...</option>
                {collections
                  .filter((c) => c.info.source_dir)
                  .map((c) => (
                    <option key={c.path} value={c.info.source_dir!}>
                      {c.info.name}
                    </option>
                  ))}
              </select>
              <button
                className="drift-check-btn"
                disabled={driftLoading || !driftProjectDir}
                onClick={async () => {
                  if (!driftProjectDir) return;
                  setDriftLoading(true);
                  try {
                    const report = await invoke<DriftReport>("check_drift");
                    setDriftReport(report ?? { items: [], new_count: 0, stale_count: 0, changed_count: 0 });
                  } catch (err) {
                    setDriftReport(null);
                    setError(typeof err === "string" ? err : String(err));
                  } finally {
                    setDriftLoading(false);
                  }
                }}
              >
                {driftLoading ? "Checking..." : "Check Drift"}
              </button>
            </div>
            {collections.filter((c) => c.info.source_dir).length === 0 && (
              <p className="placeholder">No collections from codebase scan</p>
            )}
            {driftReport && !driftReport.items.length && (
              <div className="drift-no-drift">No drift detected</div>
            )}
            {driftReport && driftReport.items.length > 0 && (
              <div className="drift-results">
                <div className="drift-summary">
                  {driftReport.new_count > 0 && (
                    <span className="drift-badge drift-new">+{driftReport.new_count} new</span>
                  )}
                  {driftReport.stale_count > 0 && (
                    <span className="drift-badge drift-stale">-{driftReport.stale_count} stale</span>
                  )}
                  {driftReport.changed_count > 0 && (
                    <span className="drift-badge drift-changed">~{driftReport.changed_count} changed</span>
                  )}
                  <button
                    className="drift-sync-btn"
                    disabled={driftLoading}
                    onClick={async () => {
                      setDriftLoading(true);
                      try {
                        const report = await invoke<DriftReport>("fix_drift");
                        setDriftReport(report);
                        // Refresh collection sidebar
                        if (activeCollectionPath) {
                          const info = await invoke<IpcCollectionInfo>("open_collection", {
                            wireDir: activeCollectionPath,
                          });
                          setCollections((prev) =>
                            prev.map((c) =>
                              c.path === activeCollectionPath ? { info, path: c.path } : c
                            )
                          );
                        }
                      } catch (err) {
                        setError(typeof err === "string" ? err : String(err));
                      } finally {
                        setDriftLoading(false);
                      }
                    }}
                  >
                    {driftLoading ? "Syncing..." : "Sync All"}
                  </button>
                </div>
                {driftReport.items.map((item, i) => (
                  <div key={i} className={`drift-item drift-${item.category}`}>
                    <div className="drift-item-header">
                      <span className={`drift-category drift-cat-${item.category}`}>
                        {item.category === "new" ? "+" : item.category === "stale" ? "-" : "~"}
                      </span>
                      <span className="method-badge" style={{
                        color: item.method === "GET" ? "#4ec9b0" : item.method === "POST" ? "#dcdcaa" : item.method === "DELETE" ? "#f44747" : "#569cd6"
                      }}>
                        {item.method}
                      </span>
                      <span className="drift-route">{item.route}</span>
                    </div>
                    <div className="drift-item-name">{item.name}</div>
                    {(item.changes ?? []).map((c, j) => (
                      <div key={j} className="drift-change">{c}</div>
                    ))}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </aside>

      {/* Center Panel: Request Builder */}
      <main className="request-builder">
        {isEditingTemplate ? (
        <div className="template-header-bar">
          <span className="template-header-icon">T</span>
          <span className="template-header-name">{selectedRequestName ?? "Template"}</span>
          <button
            className="save-btn"
            onClick={async () => {
              if (!activeCollectionPath || !selectedRequestName) {
                setError("No collection or template selected");
                return;
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
                if (bodyText.trim()) {
                  try {
                    body = { type: "json", content: JSON.parse(bodyText) };
                  } catch {
                    body = { type: "text", content: bodyText };
                  }
                }
                const params: Record<string, string> = {};
                for (const p of queryParams) {
                  if (p.key.trim() && p.enabled !== false) {
                    params[p.key.trim()] = p.value;
                  }
                }
                const tmpl: WireRequest = {
                  name: selectedRequestName,
                  method: "",
                  url: "",
                  headers,
                  params,
                  body,
                  tests: currentAssertions.length > 0 ? currentAssertions : undefined,
                };
                await invoke("save_template", { name: selectedRequestName, request: tmpl });
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
            }}
          >
            Save Template
          </button>
        </div>
        ) : (
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
                            resolved !== undefined && resolved !== ""
                              ? resolved
                              : "Not Set"
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
          {activeCollectionPath && (
            <button
              className="save-template-btn"
              onClick={async () => {
                const name = await showPrompt("Template name:", "");
                if (!name?.trim()) return;
                const tmplName = name.trim().replace(/\s+/g, "-").toLowerCase();
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
                  const params: Record<string, string> = {};
                  for (const p of queryParams) {
                    if (p.key.trim() && p.enabled !== false) {
                      params[p.key.trim()] = p.value;
                    }
                  }
                  const tmpl: WireRequest = {
                    name: tmplName,
                    method: "",
                    url: "",
                    headers,
                    params,
                    body,
                  };
                  await invoke("save_template", { name: tmplName, request: tmpl });
                  // Refresh collection to pick up new template
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
              }}
            >
              Save as Template
            </button>
          )}
        </div>
        )}
        {!isEditingTemplate && activeTemplates.length > 0 && (
          <div className="template-picker">
            <span className="template-picker-label">Template:</span>
            <select
              className="template-select"
              value={extendsTemplate ?? ""}
              onChange={(e) => {
                const val = e.target.value === "" ? null : e.target.value;
                setExtendsTemplate(val);
                if (val) {
                  invoke<WireRequest>("read_template", { name: val })
                    .then((tmpl) => {
                      const parts: string[] = [];
                      const headerNames = Object.keys(tmpl.headers);
                      if (headerNames.length > 0) parts.push(...headerNames);
                      const paramNames = Object.keys(tmpl.params);
                      if (paramNames.length > 0) parts.push(...paramNames.map((p) => `${p} param`));
                      if (tmpl.body) parts.push("body");
                      setExtendsTooltip(parts.length > 0 ? `Inheriting ${parts.join(", ")}` : "");
                    })
                    .catch(() => setExtendsTooltip(""));
                } else {
                  setExtendsTooltip("");
                }
              }}
            >
              <option value="">(none)</option>
              {activeTemplates.map((t) => (
                <option key={t} value={t}>{t}</option>
              ))}
            </select>
            {extendsTooltip && (
              <span className="template-picker-hint">{extendsTooltip}</span>
            )}
          </div>
        )}
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
              {testResults.length > 0 && (
                <span
                  className={`tab-badge ${testResults.every((r) => r.passed) ? "tab-badge-pass" : "tab-badge-fail"}`}
                >
                  {testResults.filter((r) => r.passed).length}/{testResults.length}
                </span>
              )}
              {testResults.length === 0 && currentAssertions.length > 0 && (
                <span className="tab-badge tab-badge-pending">
                  {currentAssertions.length}
                </span>
              )}
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
              <div className="query-params-editor">
                <h3 className="query-params-title">Query Parameters</h3>
                {(queryParams.length > 0
                  ? queryParams
                  : [{ key: "", value: "", enabled: true }]
                ).map((param, i) => (
                  <div key={i} className="query-param-row">
                    <input
                      type="checkbox"
                      className="query-param-checkbox"
                      checked={param.enabled !== false}
                      onChange={(e) => {
                        const updated = [...queryParams];
                        if (i >= updated.length) {
                          updated.push({ key: "", value: "", enabled: e.target.checked });
                        } else {
                          updated[i] = { ...updated[i], enabled: e.target.checked };
                        }
                        setQueryParams(updated);
                      }}
                    />
                    <input
                      className="query-param-key"
                      type="text"
                      placeholder="parameter"
                      value={param.key}
                      onChange={(e) => {
                        const updated = queryParams.length > 0 ? [...queryParams] : [{ key: "", value: "", enabled: true }];
                        updated[i] = { ...updated[i], key: e.target.value };
                        // Auto-add new empty row when typing in the last row
                        if (i === updated.length - 1 && e.target.value) {
                          updated.push({ key: "", value: "", enabled: true });
                        }
                        setQueryParams(updated);
                      }}
                    />
                    <input
                      className="query-param-value"
                      type="text"
                      placeholder="value"
                      value={param.value}
                      onChange={(e) => {
                        const updated = queryParams.length > 0 ? [...queryParams] : [{ key: "", value: "", enabled: true }];
                        updated[i] = { ...updated[i], value: e.target.value };
                        setQueryParams(updated);
                      }}
                    />
                  </div>
                ))}
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
              <div className="test-results-panel">
                <div className="test-editor">
                  <h3 className="test-editor-title">Test Assertions</h3>
                  {currentAssertions.map((assertion, i) => {
                    const op = getAssertionOperator(assertion);
                    const val = getAssertionValue(assertion);
                    const result = testResults[i];
                    return (
                      <div
                        key={i}
                        className={`test-assertion-row ${result ? (result.passed ? "passed" : "failed") : ""}`}
                      >
                        {result && (
                          <span className="test-result-icon">
                            {result.passed ? "\u2713" : "\u2717"}
                          </span>
                        )}
                        <input
                          className="test-assertion-field"
                          type="text"
                          placeholder="field (e.g. status, body.name)"
                          value={assertion.field}
                          onChange={(e) => {
                            const updated = [...currentAssertions];
                            updated[i] = { ...updated[i], field: e.target.value };
                            setCurrentAssertions(updated);
                          }}
                        />
                        <select
                          className="test-assertion-operator"
                          value={op}
                          onChange={(e) => {
                            const updated = [...currentAssertions];
                            updated[i] = buildAssertion(
                              updated[i].field,
                              e.target.value,
                              val
                            );
                            setCurrentAssertions(updated);
                          }}
                        >
                          <option value="equals">equals</option>
                          <option value="not_equals">not_equals</option>
                          <option value="contains">contains</option>
                          <option value="starts_with">starts_with</option>
                          <option value="ends_with">ends_with</option>
                          <option value="less_than">less_than</option>
                          <option value="greater_than">greater_than</option>
                          <option value="is_array">is_array</option>
                          <option value="is_object">is_object</option>
                          <option value="is_string">is_string</option>
                          <option value="is_number">is_number</option>
                          <option value="exists">exists</option>
                          <option value="body_contains">body_contains</option>
                          <option value="body_matches">body_matches</option>
                        </select>
                        <input
                          className="test-assertion-value"
                          type="text"
                          placeholder="expected value"
                          value={val}
                          onChange={(e) => {
                            const updated = [...currentAssertions];
                            updated[i] = buildAssertion(
                              updated[i].field,
                              op,
                              e.target.value
                            );
                            setCurrentAssertions(updated);
                          }}
                        />
                        <button
                          className="test-assertion-remove"
                          onClick={() =>
                            setCurrentAssertions(
                              currentAssertions.filter((_, j) => j !== i)
                            )
                          }
                        >
                          &#x2715;
                        </button>
                        {result && !result.passed && (
                          <span className="test-result-actual">
                            got {result.actual}
                          </span>
                        )}
                      </div>
                    );
                  })}
                  <div className="test-assertion-actions">
                    <button
                      className="test-assertion-add"
                      onClick={() =>
                        setCurrentAssertions([
                          ...currentAssertions,
                          { field: "status", equals: 200 },
                        ])
                      }
                    >
                      + Add Assertion
                    </button>
                    {response && (
                      <div className="response-fields-picker">
                        <button
                          className="test-assertion-add test-assertion-from-response"
                          onClick={() => setResponseFieldsOpen(!responseFieldsOpen)}
                        >
                          + Add from Response &#x25BE;
                        </button>
                        {responseFieldsOpen && (
                          <>
                            <div
                              className="dropdown-backdrop"
                              onClick={() => setResponseFieldsOpen(false)}
                            />
                            <div className="response-fields-dropdown">
                              {extractResponseFields().map((field, i) => (
                                <button
                                  key={i}
                                  className="response-field-item"
                                  onClick={() => {
                                    const num = Number(field.value);
                                    const val = !isNaN(num) && field.value.trim() !== ""
                                      ? num
                                      : field.value;
                                    setCurrentAssertions([
                                      ...currentAssertions,
                                      { field: field.path, equals: val },
                                    ]);
                                    setResponseFieldsOpen(false);
                                  }}
                                >
                                  <span className="response-field-path">{field.path}</span>
                                  <span className="response-field-value">{field.value}</span>
                                </button>
                              ))}
                            </div>
                          </>
                        )}
                      </div>
                    )}
                    {responseSchema.length > 0 && (
                      <div className="response-fields-picker">
                        <button
                          className="test-assertion-add test-assertion-from-schema"
                          onClick={() => {
                            // Add all schema fields as assertions at once
                            const newAssertions = responseSchema.map(([name, typeHint]) => {
                              const field = `body.${name.charAt(0).toLowerCase() + name.slice(1)}`;
                              const boolTypes = ["is_array", "is_object", "is_string", "is_number"];
                              const typeMap: Record<string, string> = {
                                "string": "is_string",
                                "string?": "is_string",
                                "int": "is_number",
                                "long": "is_number",
                                "double": "is_number",
                                "decimal": "is_number",
                                "float": "is_number",
                                "bool": "exists",
                                "Guid": "is_string",
                                "DateTime": "is_string",
                                "DateTime?": "is_string",
                              };
                              const op = typeMap[typeHint] ?? (typeHint.startsWith("List<") ? "is_array" : "exists");
                              const a: Assertion = { field };
                              if (boolTypes.includes(op)) {
                                (a as Record<string, unknown>)[op] = true;
                              } else {
                                a.exists = true;
                              }
                              return a;
                            });
                            setCurrentAssertions([...currentAssertions, ...newAssertions]);
                          }}
                        >
                          + Add from Schema ({responseSchema.length} fields)
                        </button>
                      </div>
                    )}
                  </div>
                </div>
                {testResults.length > 0 && (
                  <div className="test-results-summary-bar">
                    <span className="test-pass-count">
                      {testResults.filter((r) => r.passed).length} passed
                    </span>
                    {testResults.some((r) => !r.passed) && (
                      <span className="test-fail-count">
                        {testResults.filter((r) => !r.passed).length} failed
                      </span>
                    )}
                  </div>
                )}
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
