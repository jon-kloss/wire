import { useState, useCallback, useEffect, useRef, lazy, Suspense } from "react";
import { invoke, open, isWebPlayground } from "./api/invoke";
import { SampleGallery } from "./SampleGallery";
import { SourceEditor } from "./SourceEditor";
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
  DriftItem,
  ChainResult,
  ChainStepDef,
  SnapshotComparison,
  BreakingReport,
  SecretCheckResult,
  DiffEntry,
} from "./types";
import type { TreeNode } from "./utils";
import {
  buildTree,
  filterTree,
  formatTimeAgo,
  METHOD_COLORS,
  METHOD_COLOR_FALLBACK,
  statusColor,
  formatBody,
  formatBytes,
} from "./utils";
import { PromptModal } from "./PromptModal";
import { useAccent } from "./accent";
import { WireLockup, WireMark, AccentPicker } from "./brand";
import { defineWireMonacoTheme, WIRE_MONACO_THEME } from "./monacoTheme";
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

/** Map a drift category to a display severity (breaking / warning / info). */
function driftSeverity(category: "new" | "stale" | "changed"): {
  label: string;
  cls: string;
  glyph: string;
} {
  if (category === "stale") return { label: "BREAKING", cls: "breaking", glyph: "✗" };
  if (category === "changed") return { label: "WARNING", cls: "warning", glyph: "~" };
  return { label: "INFO", cls: "info", glyph: "+" };
}

/** Stable identity for a drift item (survives re-checks / ignore filtering). */
function driftKey(item: DriftItem): string {
  return `${item.category}:${item.method}:${item.route}`;
}

/** Editable chain step (the persisted `extract` Record is rebuilt from rows). */
type ChainDraftStep = {
  run: string;
  extracts: Array<{ name: string; path: string }>;
  persist: boolean;
};

function stepToDraft(s: ChainStepDef): ChainDraftStep {
  return {
    run: s.run,
    extracts: Object.entries(s.extract ?? {}).map(([name, path]) => ({
      name,
      path,
    })),
    persist: s.persist ?? false,
  };
}

function draftToStep(d: ChainDraftStep): ChainStepDef {
  const extract: Record<string, string> = {};
  for (const e of d.extracts) if (e.name.trim()) extract[e.name.trim()] = e.path;
  const step: ChainStepDef = { run: d.run };
  if (Object.keys(extract).length) step.extract = extract;
  if (d.persist) step.persist = true;
  return step;
}

/** Compact display of a snapshot diff entry's value(s). */
function diffValueText(e: DiffEntry): string {
  const fmt = (v: unknown) => {
    if (v === undefined) return "";
    const s = typeof v === "string" ? v : JSON.stringify(v);
    return s.length > 48 ? s.slice(0, 45) + "…" : s;
  };
  if (e.kind === "Changed") return `${fmt(e.old)} → ${fmt(e.new)}`;
  if (e.kind === "Added") return fmt(e.new);
  return fmt(e.old);
}

function TreeItem({
  node,
  depth,
  onSelect,
  onDelete,
  selectedPath,
  defaultExpanded = true,
}: {
  node: TreeNode;
  depth: number;
  onSelect: (entry: IpcRequestEntry) => void;
  onDelete?: (entry: IpcRequestEntry) => void;
  selectedPath: string | null;
  defaultExpanded?: boolean;
}) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  useEffect(() => { setExpanded(defaultExpanded); }, [defaultExpanded]);
  const isFolder = !node.entry && node.children.size > 0;
  const isSelected = node.entry?.path === selectedPath;

  if (node.entry) {
    const color = METHOD_COLORS[node.entry.method] ?? METHOD_COLOR_FALLBACK;
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
        {onDelete && (
          <button
            className="tree-delete"
            title="Delete request"
            onClick={(e) => {
              e.stopPropagation();
              onDelete(node.entry!);
            }}
          >
            &#x2715;
          </button>
        )}
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
              onDelete={onDelete}
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
  const filterInputRef = useRef<HTMLInputElement>(null);

  // Accent theming (Ember / Signal / Pulse) — persisted, applied to <html>
  const { accent, setAccent } = useAccent();

  // Title bar env switcher popover
  const [envMenuOpen, setEnvMenuOpen] = useState(false);

  // Request builder state
  const [method, setMethod] = useState("GET");
  const [url, setUrl] = useState("");
  const [headersText, setHeadersText] = useState("");
  const [bodyText, setBodyText] = useState("");
  const [bodyType, setBodyType] = useState<"json" | "text" | "formdata">("json");
  const [formDataPairs, setFormDataPairs] = useState<
    Array<{ key: string; value: string; enabled?: boolean }>
  >([]);
  const [queryParams, setQueryParams] = useState<Array<{ key: string; value: string; enabled?: boolean }>>([]);
  const [activeTab, setActiveTab] = useState<
    "query" | "headers" | "auth" | "body" | "tests" | "chain" | "pre-run"
  >("query");

  // Response state
  const [response, setResponse] = useState<IpcResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [testResults, setTestResults] = useState<TestResult[]>([]);
  const [currentAssertions, setCurrentAssertions] = useState<Assertion[]>([]);
  const [responseSchema, setResponseSchema] = useState<[string, string][]>([]);
  const [error, setError] = useState<string | null>(null);
  const [responseTab, setResponseTab] = useState<"body" | "headers" | "tests">("body");
  const [snapshotComparison, setSnapshotComparison] = useState<SnapshotComparison | null>(null);
  const [snapshotMsg, setSnapshotMsg] = useState<string | null>(null);

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
  const [newVarKey, setNewVarKey] = useState("");
  const [newVarValue, setNewVarValue] = useState("");
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

  // Breadcrumb segments for the title bar: collection / folder… / request
  const breadcrumb: string[] = (() => {
    if (!activeCollection) return [];
    const segs: string[] = [activeCollection.info.name];
    if (selectedRequestPath) {
      const prefix = activeCollection.path + "/requests/";
      let rel = selectedRequestPath.startsWith(prefix)
        ? selectedRequestPath.slice(prefix.length)
        : selectedRequestPath.split("/").pop() ?? "";
      rel = rel.replace(/\.wire\.yaml$/, "");
      const parts = rel.split("/");
      for (let i = 0; i < parts.length - 1; i++) segs.push(parts[i]);
      segs.push(selectedRequestName ?? parts[parts.length - 1]);
    }
    return segs;
  })();

  // Drift state
  const [driftReport, setDriftReport] = useState<DriftReport | null>(null);
  const [driftLoading, setDriftLoading] = useState(false);
  const [driftProjectDir, setDriftProjectDir] = useState("");
  const [sourceEditorDir, setSourceEditorDir] = useState<string | null>(null);
  const [selectedDriftIdx, setSelectedDriftIdx] = useState<number | null>(null);
  const [ignoredDrift, setIgnoredDrift] = useState<Set<string>>(new Set());

  // Secret-check state (wire env check)
  const [secretCheck, setSecretCheck] = useState<SecretCheckResult[] | null>(null);
  const [secretCheckLoading, setSecretCheckLoading] = useState(false);

  // Breaking-change state (wire breaking)
  const [breakingReport, setBreakingReport] = useState<BreakingReport | null>(null);
  const [breakingLoading, setBreakingLoading] = useState(false);
  const [driftMode, setDriftMode] = useState<"drift" | "breaking">("drift");

  // Chain state
  const [chainSteps, setChainSteps] = useState<ChainStepDef[]>([]);
  const [chainDraft, setChainDraft] = useState<ChainDraftStep[]>([]);
  const [chainResult, setChainResult] = useState<ChainResult | null>(null);
  const [chainLoading, setChainLoading] = useState(false);
  const [expandedChainSteps, setExpandedChainSteps] = useState<Set<number>>(new Set());

  // Secret masking state
  const [secretsRevealed, setSecretsRevealed] = useState(false);

  // Scratch mode: show the workspace grid even with no collection open
  // (entered from the onboarding quick-send / new-request affordances).
  const [scratchMode, setScratchMode] = useState(false);
  const [onboardUrl, setOnboardUrl] = useState("");

  /** Check if a raw env var value is a secret reference */
  const isSecretValue = (val: string) =>
    val.startsWith("$env:") || val.startsWith("$dotenv:") || val.startsWith("$aws:") || val.startsWith("$vault:");

  // History state
  const [history, setHistory] = useState<HistoryEntry[]>([]);

  // Delete-request confirmation
  const [confirmDelete, setConfirmDelete] = useState<{
    path: string;
    name: string;
  } | null>(null);

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

  const handleClearHistory = useCallback(async () => {
    try {
      await invoke("clear_history");
      setHistory([]);
    } catch (err) {
      setError(String(err));
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

  /** Select an environment for the active collection and load its vars. */
  const selectEnv = useCallback(
    (val: string | null) => {
      if (!activeCollectionPath) return;
      const path = activeCollectionPath;
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
    },
    [activeCollectionPath]
  );

  /** Sync the collection to source (apply all drift) and refresh the sidebar. */
  const fixDriftAll = useCallback(async () => {
    setDriftLoading(true);
    try {
      const report = await invoke<DriftReport>("fix_drift");
      setDriftReport(report);
      setSelectedDriftIdx(null);
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
  }, [activeCollectionPath]);

  /** Validate the active collection's secret references (wire env check). */
  const handleCheckSecrets = useCallback(
    async (collectionPath: string) => {
      if (collectionPath !== activeCollectionPath) {
        try {
          await invoke("open_collection", { wireDir: collectionPath });
          setActiveCollectionPath(collectionPath);
        } catch {
          /* will surface below */
        }
      }
      setSecretCheckLoading(true);
      try {
        const results = await invoke<SecretCheckResult[]>("check_secrets");
        setSecretCheck(results);
      } catch (err) {
        setError(String(err));
      } finally {
        setSecretCheckLoading(false);
      }
    },
    [activeCollectionPath]
  );

  /** Save the current collection contract as the breaking-change baseline. */
  const handleSaveBaseline = useCallback(async () => {
    setBreakingLoading(true);
    try {
      await invoke<number>("save_breaking_baseline");
      const report = await invoke<BreakingReport>("check_breaking");
      setBreakingReport(report);
      setDriftMode("breaking");
    } catch (err) {
      setError(String(err));
    } finally {
      setBreakingLoading(false);
    }
  }, []);

  /** Compare the collection contract against the saved baseline. */
  const handleCheckBreaking = useCallback(async () => {
    setBreakingLoading(true);
    setDriftMode("breaking");
    try {
      const report = await invoke<BreakingReport>("check_breaking");
      setBreakingReport(report);
    } catch {
      // Most likely no baseline yet — the empty state guides the user to save one.
      setBreakingReport(null);
    } finally {
      setBreakingLoading(false);
    }
  }, []);

  /** Create a new (empty) environment file in a collection. */
  const handleCreateEnvironment = useCallback(
    async (collectionPath: string) => {
      const name = await showPrompt("New environment name:", "");
      if (!name?.trim()) return;
      const envName = name.trim();
      try {
        await invoke("save_environment", {
          wireDir: collectionPath,
          envName,
          variables: {},
        });
        const info = await invoke<IpcCollectionInfo>("open_collection", {
          wireDir: collectionPath,
        });
        setCollections((prev) =>
          prev.map((c) =>
            c.path === collectionPath ? { info, path: c.path } : c
          )
        );
        setActiveCollectionPath(collectionPath);
        setExpandedEnvSections((prev) => new Set(prev).add(collectionPath));
        setEnvSelectedMap((prev) => ({ ...prev, [collectionPath]: envName }));
        setEnvVarsMap((prev) => ({ ...prev, [collectionPath]: {} }));
      } catch (err) {
        setError(String(err));
      }
    },
    [showPrompt]
  );

  /** Add a new variable to the selected environment. */
  const handleAddVariable = useCallback(
    (collectionPath: string, envName: string) => {
      const key = newVarKey.trim();
      if (!key) return;
      handleSaveEnvVar(collectionPath, envName, key, newVarValue);
      setNewVarKey("");
      setNewVarValue("");
    },
    [newVarKey, newVarValue, handleSaveEnvVar]
  );

  const handleNewRequest = useCallback(() => {
    setMethod("GET");
    setUrl("");
    setHeadersText("");
    setBodyText("");
    setBodyType("json");
    setFormDataPairs([]);
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
    setSnapshotComparison(null);
    setChainSteps([]);
    setChainDraft([]);
    setChainResult(null);
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

  /** Delete a request .wire.yaml from disk and refresh its collection. */
  const handleDeleteRequest = useCallback(
    async (filePath: string) => {
      try {
        await invoke("delete_request", { file: filePath });
        const owner = collections.find((c) => filePath.startsWith(c.path));
        if (owner) {
          const info = await invoke<IpcCollectionInfo>("open_collection", {
            wireDir: owner.path,
          });
          setCollections((prev) =>
            prev.map((c) => (c.path === owner.path ? { info, path: c.path } : c))
          );
        }
        if (selectedRequestPath === filePath) {
          handleNewRequest();
        }
      } catch (err) {
        setError(String(err));
      }
    },
    [collections, selectedRequestPath, handleNewRequest]
  );

  /** Build the request body from the current body-type + editors. */
  const buildBody = useCallback(
    (m: string): WireBody | null => {
      if (!["POST", "PUT", "PATCH"].includes(m)) return null;
      if (bodyType === "formdata") {
        const content: Record<string, string> = {};
        for (const p of formDataPairs) {
          if (p.key.trim() && p.enabled !== false) content[p.key.trim()] = p.value;
        }
        return Object.keys(content).length
          ? { type: "formdata", content }
          : null;
      }
      if (!bodyText.trim()) return null;
      if (bodyType === "text") return { type: "text", content: bodyText };
      try {
        return { type: "json", content: JSON.parse(bodyText) };
      } catch {
        return { type: "text", content: bodyText };
      }
    },
    [bodyType, bodyText, formDataPairs]
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

      const body = buildBody(method);

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
        chain: chainSteps.length > 0 ? chainSteps : undefined,
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
  }, [method, url, headersText, buildBody, queryParams, currentAssertions, chainSteps, activeCollectionPath, selectedRequestPath, selectedRequestName, extendsTemplate, showPrompt]);

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

        // Build body editors from the request's body
        if (req.body) {
          setBodyType(req.body.type);
          if (req.body.type === "json") {
            setBodyText(JSON.stringify(req.body.content, null, 2));
            setFormDataPairs([]);
          } else if (req.body.type === "formdata") {
            const content = (req.body.content ?? {}) as Record<string, unknown>;
            setFormDataPairs(
              Object.entries(content).map(([key, value]) => ({
                key,
                value: String(value),
                enabled: true,
              }))
            );
            setBodyText("");
          } else {
            setBodyText(String(req.body.content));
            setFormDataPairs([]);
          }
        } else {
          setBodyText("");
          setBodyType("json");
          setFormDataPairs([]);
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
        setChainSteps(req.chain ?? []);
        setChainDraft((req.chain ?? []).map(stepToDraft));
        setChainResult(null);
        setSnapshotComparison(null);
      } catch (err) {
        setError(String(err));
      }
    },
    []
  );

  const handleSend = async (overrideUrl?: string, overrideMethod?: string) => {
    const effUrl = overrideUrl ?? url;
    const effMethod = overrideMethod ?? method;
    if (!effUrl.trim()) return;

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

      const body = buildBody(effMethod);

      const params: Record<string, string> = {};
      for (const p of queryParams) {
        if (p.key.trim() && p.enabled !== false) {
          params[p.key.trim()] = p.value;
        }
      }

      const request: WireRequest = {
        name: "Quick Request",
        method: effMethod,
        url: effUrl,
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

      // Compare against a saved golden-file snapshot, if this is a saved request
      if (selectedRequestPath) {
        invoke<SnapshotComparison>("compare_response_snapshot", {
          file: selectedRequestPath,
          response: result,
        })
          .then(setSnapshotComparison)
          .catch(() => setSnapshotComparison(null));
      } else {
        setSnapshotComparison(null);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  /** Save the current response as this request's golden-file snapshot. */
  const handleSaveSnapshot = useCallback(async () => {
    if (!selectedRequestPath || !response) return;
    try {
      await invoke("save_response_snapshot", {
        file: selectedRequestPath,
        response,
      });
      setSnapshotMsg("Snapshot saved");
      const cmp = await invoke<SnapshotComparison>("compare_response_snapshot", {
        file: selectedRequestPath,
        response,
      });
      setSnapshotComparison(cmp);
      window.setTimeout(() => setSnapshotMsg(null), 2500);
    } catch (err) {
      setError(String(err));
    }
  }, [selectedRequestPath, response]);

  /** Update the chain draft and keep the runnable chainSteps in sync. */
  const updateChainDraft = useCallback((draft: ChainDraftStep[]) => {
    setChainDraft(draft);
    setChainSteps(draft.filter((d) => d.run).map(draftToStep));
  }, []);

  const handleRunChain = async () => {
    if (!selectedRequestPath || chainSteps.length === 0) return;

    setChainLoading(true);
    setChainResult(null);
    setExpandedChainSteps(new Set());
    setError(null);

    try {
      const result = await invoke<ChainResult>("run_chain", {
        file: selectedRequestPath,
        env: activeSelectedEnv,
      });
      setChainResult(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setChainLoading(false);
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
      (a as unknown as Record<string, unknown>)[operator] = value !== "false";
    } else if (numOps.includes(operator)) {
      (a as unknown as Record<string, unknown>)[operator] = parseFloat(value) || 0;
    } else if (operator === "equals" || operator === "not_equals") {
      const num = Number(value);
      (a as unknown as Record<string, unknown>)[operator] = !isNaN(num) && value.trim() !== "" ? num : value;
    } else {
      (a as unknown as Record<string, unknown>)[operator] = value;
    }
    return a;
  };

  return (
    <div className="app-shell">
      <SampleGallery />
      {sourceEditorDir && (
        <SourceEditor
          sourceDir={sourceEditorDir}
          wireDir={
            collections.find((c) => c.info.source_dir === sourceEditorDir)?.path ?? null
          }
          onClose={() => setSourceEditorDir(null)}
          onDrift={(report) => setDriftReport(report)}
        />
      )}

      {/* Title bar */}
      <header className="titlebar">
        <WireLockup />
        <span className="tb-divider" />
        <nav className="breadcrumb">
          {breadcrumb.length === 0 ? (
            <span className="breadcrumb-empty">No collection open</span>
          ) : (
            breadcrumb.map((seg, i) => (
              <span key={i} className="breadcrumb-part">
                {i > 0 && <span className="breadcrumb-sep">/</span>}
                <span
                  className={`breadcrumb-seg ${i === breadcrumb.length - 1 ? "leaf" : ""}`}
                >
                  {seg}
                </span>
              </span>
            ))
          )}
        </nav>
        <div className="tb-spacer" />
        <AccentPicker accent={accent} onPick={setAccent} />
        {activeCollection && activeCollection.info.environments.length > 0 && (
          <div className="env-switcher">
            <button
              className="env-chip"
              onClick={() => setEnvMenuOpen((o) => !o)}
              title="Switch environment"
            >
              <span className="env-dot" />
              <span className="env-chip-name">{activeSelectedEnv ?? "no env"}</span>
              <span className="env-chip-caret">{"▾"}</span>
            </button>
            {envMenuOpen && (
              <>
                <div
                  className="dropdown-backdrop"
                  onClick={() => setEnvMenuOpen(false)}
                />
                <div className="env-menu">
                  <button
                    className={`env-menu-item ${!activeSelectedEnv ? "active" : ""}`}
                    onClick={() => {
                      selectEnv(null);
                      setEnvMenuOpen(false);
                    }}
                  >
                    (no env)
                  </button>
                  {activeCollection.info.environments.map((env) => (
                    <button
                      key={env}
                      className={`env-menu-item ${activeSelectedEnv === env ? "active" : ""}`}
                      onClick={() => {
                        selectEnv(env);
                        setEnvMenuOpen(false);
                      }}
                    >
                      {env}
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>
        )}
        <button
          className="tb-icon-btn"
          title="Search requests"
          onClick={() => {
            setSidebarTab("collections");
            setTimeout(() => filterInputRef.current?.focus(), 0);
          }}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
            <circle cx="11" cy="11" r="7" />
            <path d="M21 21l-4.3-4.3" />
          </svg>
        </button>
        <button
          className={`tb-icon-btn ${secretsRevealed ? "active" : ""}`}
          title={secretsRevealed ? "Hide secret values" : "Reveal secret values"}
          onClick={() => setSecretsRevealed((v) => !v)}
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <rect x="4" y="11" width="16" height="10" rx="2" />
            <path d={secretsRevealed ? "M8 11V7a4 4 0 0 1 7.5-2" : "M8 11V7a4 4 0 0 1 8 0v4"} />
          </svg>
        </button>
      </header>

      {collections.length === 0 && !scratchMode ? (
      <div className="onboarding">
        <div className="onboard-grid">
          <div className="onboard-left">
            <div className="onboard-lockup">
              <WireMark size={36} />
              <span className="onboard-wordmark">
                W<span className="onboard-i">i</span>re
              </span>
              <span className="onboard-version">v0.1</span>
            </div>
            <h2 className="onboard-title">Your API, as files you actually own.</h2>
            <p className="onboard-sub">
              Every request, test, and environment lives as a{" "}
              <span className="onboard-code">.wire.yaml</span> file in your repo —
              versioned, diffable, and readable by humans and agents alike.
            </p>

            <div className="onboard-quicksend">
              <span className="onboard-qs-method">GET</span>
              <input
                className="onboard-qs-url"
                type="text"
                placeholder="https://api.example.com/health"
                value={onboardUrl}
                onChange={(e) => setOnboardUrl(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    const u = onboardUrl.trim();
                    if (!u) return;
                    setScratchMode(true);
                    setMethod("GET");
                    setUrl(u);
                    handleSend(u, "GET");
                  }
                }}
              />
              <button
                className="onboard-qs-send"
                onClick={() => {
                  const u = onboardUrl.trim();
                  if (!u) return;
                  setScratchMode(true);
                  setMethod("GET");
                  setUrl(u);
                  handleSend(u, "GET");
                }}
              >
                Send
              </button>
            </div>

            <div className="onboard-cards">
              <button className="onboard-card" onClick={handleOpenCollection}>
                <span className="onboard-card-icon">{"\u{1F4C2}"}</span>
                <span className="onboard-card-text">
                  <span className="onboard-card-title">Open collection</span>
                  <span className="onboard-card-sub">
                    Load an existing <code>.wire/</code> directory
                  </span>
                </span>
                <span className="onboard-card-kbd">{"⌘O"}</span>
              </button>
              <button
                className="onboard-card"
                onClick={handleImportFromCodebase}
              >
                <span className="onboard-card-icon">{"⚡"}</span>
                <span className="onboard-card-text">
                  <span className="onboard-card-title">Generate from codebase</span>
                  <span className="onboard-card-sub">
                    Scan source for endpoints
                  </span>
                </span>
                <span className="onboard-card-kbd">{"⌘G"}</span>
              </button>
              <button className="onboard-card" onClick={handleNewCollection}>
                <span className="onboard-card-icon">{"➕"}</span>
                <span className="onboard-card-text">
                  <span className="onboard-card-title">New collection</span>
                  <span className="onboard-card-sub">Start from scratch</span>
                </span>
                <span className="onboard-card-kbd">{"⌘N"}</span>
              </button>
            </div>

            {history.length > 0 && (
              <div className="onboard-recent">
                <span className="onboard-recent-label">RECENT</span>
                <div className="onboard-recent-chips">
                  {history.slice(0, 4).map((h, i) => (
                    <button
                      key={`${h.timestamp}-${i}`}
                      className="onboard-recent-chip"
                      title={h.url}
                      onClick={() => {
                        setScratchMode(true);
                        setMethod(h.method);
                        setUrl(h.url);
                      }}
                    >
                      <span
                        className="method-badge"
                        style={{
                          color:
                            METHOD_COLORS[h.method] ?? METHOD_COLOR_FALLBACK,
                        }}
                      >
                        {h.method}
                      </span>
                      <span className="onboard-recent-name">
                        {h.name || h.url}
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            )}

            <div className="onboard-claude">
              <span className="onboard-claude-label">WORKS WITH CLAUDE CODE</span>
              <code className="onboard-claude-cmd">
                $ wire install-claude-skill
              </code>
            </div>
          </div>

          <div className="onboard-right">
            <div className="onboard-yaml-card">
              <div className="onboard-yaml-head">
                <span className="onboard-yaml-dot" />
                <span className="onboard-yaml-path">
                  .wire/requests/users/<span className="accent">create.wire.yaml</span>
                </span>
              </div>
              <pre className="onboard-yaml">
                <span className="yk">name</span>
                <span className="yp">:</span> <span className="ys">Create user</span>
                {"\n"}
                <span className="yk">method</span>
                <span className="yp">:</span> <span className="ys">POST</span>
                {"\n"}
                <span className="yk">url</span>
                <span className="yp">:</span>{" "}
                <span className="yv">{"{{baseUrl}}"}</span>
                <span className="ys">/users</span>
                {"\n"}
                <span className="yk">headers</span>
                <span className="yp">:</span>
                {"\n"}
                {"  "}
                <span className="yk">Content-Type</span>
                <span className="yp">:</span>{" "}
                <span className="ys">application/json</span>
                {"\n"}
                <span className="yk">body</span>
                <span className="yp">:</span>
                {"\n"}
                {"  "}
                <span className="yk">name</span>
                <span className="yp">:</span> <span className="ys">Ada Lovelace</span>
                {"\n"}
                {"  "}
                <span className="yk">email</span>
                <span className="yp">:</span>{" "}
                <span className="ys">ada@example.com</span>
                {"\n"}
                <span className="yk">tests</span>
                <span className="yp">:</span>
                {"\n"}
                {"  - "}
                <span className="yk">field</span>
                <span className="yp">:</span> <span className="ys">status</span>
                {"\n"}
                {"    "}
                <span className="yk">equals</span>
                <span className="yp">:</span> <span className="yn">201</span>
                {"\n"}
              </pre>
              <div className="onboard-yaml-caption">
                What you see in the GUI is exactly what lives in your repo.
              </div>
            </div>
          </div>
        </div>
      </div>
      ) : (
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
                ref={filterInputRef}
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
                        {(
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
                              <button
                                className="template-add-btn"
                                title="New environment"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  handleCreateEnvironment(path);
                                }}
                              >
                                +
                              </button>
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
                                {envSelectedMap[path] && (
                                    <div className="env-vars-editor">
                                      {Object.entries(envVarsMap[path] ?? {}).map(([key, value]) => {
                                        const isSecret = isSecretValue(value);
                                        return (
                                        <div key={key} className="env-var-row">
                                          <label className="env-var-label">
                                            {isSecret && <span className="secret-badge" title="Secret reference">{"\uD83D\uDD12"}</span>}
                                            {key}
                                          </label>
                                          <input
                                            className={`env-var-input ${isSecret && !secretsRevealed ? "masked" : ""}`}
                                            type={isSecret && !secretsRevealed ? "password" : "text"}
                                            value={value}
                                            onChange={(e) =>
                                              handleSaveEnvVar(path, envSelectedMap[path]!, key, e.target.value)
                                            }
                                          />
                                        </div>
                                        );
                                      })}
                                      <div className="env-var-row env-var-add">
                                        <input
                                          className="env-var-newkey"
                                          type="text"
                                          placeholder="new key"
                                          value={newVarKey}
                                          onChange={(e) => setNewVarKey(e.target.value)}
                                        />
                                        <input
                                          className="env-var-input"
                                          type="text"
                                          placeholder="value"
                                          value={newVarValue}
                                          onChange={(e) => setNewVarValue(e.target.value)}
                                          onKeyDown={(e) => {
                                            if (e.key === "Enter")
                                              handleAddVariable(
                                                path,
                                                envSelectedMap[path]!
                                              );
                                          }}
                                        />
                                        <button
                                          className="env-var-addbtn"
                                          title="Add variable"
                                          onClick={() =>
                                            handleAddVariable(
                                              path,
                                              envSelectedMap[path]!
                                            )
                                          }
                                        >
                                          +
                                        </button>
                                      </div>
                                    </div>
                                  )}
                                <div className="secret-check">
                                  <button
                                    className="secret-check-btn"
                                    disabled={secretCheckLoading}
                                    onClick={() => handleCheckSecrets(path)}
                                  >
                                    {secretCheckLoading
                                      ? "Checking…"
                                      : "Check secrets"}
                                  </button>
                                  {secretCheck &&
                                    path === activeCollectionPath &&
                                    (secretCheck.length === 0 ? (
                                      <div className="secret-check-empty">
                                        No secret references found.
                                      </div>
                                    ) : (
                                      <div className="secret-check-results">
                                        {secretCheck.map((r, i) => (
                                          <div
                                            key={i}
                                            className={`secret-check-row ${r.resolved ? "ok" : "err"}`}
                                          >
                                            <span className="secret-check-icon">
                                              {r.resolved ? "✓" : "✗"}
                                            </span>
                                            <span className="secret-check-var">
                                              {r.var_name}
                                            </span>
                                            <span className="secret-check-src">
                                              ${r.source}
                                            </span>
                                            {r.error && (
                                              <span
                                                className="secret-check-msg"
                                                title={r.error}
                                              >
                                                {r.error}
                                              </span>
                                            )}
                                          </div>
                                        ))}
                                      </div>
                                    ))}
                                </div>
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
                              onDelete={(entry) =>
                                setConfirmDelete({
                                  path: entry.path,
                                  name: entry.name,
                                })
                              }
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
            {history.length > 0 && (
              <div className="activity-bar">
                <span className="activity-count">{history.length} requests</span>
                <button className="activity-clear" onClick={handleClearHistory}>
                  Clear
                </button>
              </div>
            )}
            <div className="history-list">
              {history.length === 0 && (
                <p className="placeholder">No activity yet</p>
              )}
              {history.map((entry, i) => {
                const color = METHOD_COLORS[entry.method] ?? METHOD_COLOR_FALLBACK;
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
              {isWebPlayground() && (
                <button
                  className="drift-check-btn"
                  disabled={!driftProjectDir}
                  title="Edit the project's source and re-check drift"
                  onClick={() => setSourceEditorDir(driftProjectDir)}
                >
                  Edit Source
                </button>
              )}
            </div>
            <div className="drift-controls drift-breaking-controls">
              <span className="drift-controls-label">Contract baseline</span>
              <button
                className="drift-check-btn ghost"
                disabled={breakingLoading || !activeCollectionPath}
                title="Save the current contract as the breaking-change baseline"
                onClick={handleSaveBaseline}
              >
                Save baseline
              </button>
              <button
                className="drift-check-btn ghost"
                disabled={breakingLoading || !activeCollectionPath}
                title="Compare the contract against the saved baseline"
                onClick={handleCheckBreaking}
              >
                {breakingLoading ? "Checking…" : "Check breaking"}
              </button>
            </div>
            {collections.filter((c) => c.info.source_dir).length === 0 && (
              <p className="placeholder">No collections from codebase scan</p>
            )}
            {driftReport && !driftReport.items.length && (
              <div className="drift-no-drift">No drift detected</div>
            )}
            {driftReport && driftReport.items.length > 0 && (
              <div className="drift-sidebar-summary">
                {driftReport.stale_count > 0 && (
                  <span className="drift-mini breaking">
                    {driftReport.stale_count} breaking
                  </span>
                )}
                {driftReport.changed_count > 0 && (
                  <span className="drift-mini warning">
                    {driftReport.changed_count} changed
                  </span>
                )}
                {driftReport.new_count > 0 && (
                  <span className="drift-mini info">
                    {driftReport.new_count} new
                  </span>
                )}
                <span className="drift-sidebar-hint">shown in panel →</span>
              </div>
            )}
          </div>
        )}
      </aside>

      {sidebarTab === "drift" ? (
        <section className="driftview">
          <div className="driftview-modes">
            <button
              className={`driftview-mode ${driftMode === "drift" ? "active" : ""}`}
              onClick={() => setDriftMode("drift")}
            >
              Source drift
            </button>
            <button
              className={`driftview-mode ${driftMode === "breaking" ? "active" : ""}`}
              onClick={() => setDriftMode("breaking")}
            >
              Breaking changes
            </button>
          </div>
          {driftMode === "breaking" ? (
            !breakingReport ? (
              <div className="driftview-empty">
                <p className="placeholder">
                  No contract baseline yet. Save one to lock in the current API
                  shape, then check future changes against it.
                </p>
                <button
                  className="drift-sync-main"
                  disabled={breakingLoading || !activeCollectionPath}
                  onClick={handleSaveBaseline}
                >
                  {breakingLoading ? "Saving…" : "Save baseline"}
                </button>
              </div>
            ) : (
              <>
                <div className="driftview-strip">
                  <span className="drift-sev-pill breaking">
                    BREAKING <b>{breakingReport.breaking_count}</b>
                  </span>
                  <span className="drift-sev-pill warning">
                    WARNING <b>{breakingReport.warning_count}</b>
                  </span>
                  <span className="drift-sev-pill info">
                    INFO <b>{breakingReport.info_count}</b>
                  </span>
                  <span className="driftview-meta">
                    vs baseline {"·"} {breakingReport.changes.length} change
                    {breakingReport.changes.length !== 1 ? "s" : ""}
                  </span>
                  <div className="tb-spacer" />
                  <button
                    className="drift-sync-main"
                    disabled={breakingLoading}
                    onClick={handleSaveBaseline}
                    title="Overwrite the baseline with the current contract"
                  >
                    Update baseline
                  </button>
                </div>
                {breakingReport.changes.length === 0 ? (
                  <div className="driftview-empty">
                    <p className="placeholder">
                      ✓ No contract changes vs the saved baseline.
                    </p>
                  </div>
                ) : (
                  <div className="breaking-list">
                    {breakingReport.changes.map((c, i) => (
                      <div key={i} className={`driftview-item ${c.severity}`}>
                        <span className={`driftview-glyph ${c.severity}`}>
                          {c.severity === "breaking"
                            ? "✗"
                            : c.severity === "warning"
                              ? "⚠"
                              : "+"}
                        </span>
                        <span
                          className="method-badge"
                          style={{
                            color:
                              METHOD_COLORS[c.method] ?? METHOD_COLOR_FALLBACK,
                          }}
                        >
                          {c.method}
                        </span>
                        <span className="driftview-route">{c.route}</span>
                        <span className="breaking-desc">{c.description}</span>
                      </div>
                    ))}
                  </div>
                )}
              </>
            )
          ) : !driftReport || driftReport.items.length === 0 ? (
            <div className="driftview-empty">
              <p className="placeholder">
                {driftReport
                  ? "No drift detected — source and collection are in sync."
                  : "Select a collection in the sidebar and run Check Drift to compare source against your .wire collection."}
              </p>
            </div>
          ) : (
            (() => {
              const items = driftReport.items.filter(
                (it) => !ignoredDrift.has(driftKey(it))
              );
              if (items.length === 0) {
                return (
                  <div className="driftview-empty">
                    <p className="placeholder">All drift ignored.</p>
                  </div>
                );
              }
              const selIdx =
                selectedDriftIdx != null && selectedDriftIdx < items.length
                  ? selectedDriftIdx
                  : 0;
              const sel = items[selIdx];
              const selSev = driftSeverity(sel.category);
              return (
                <>
                  <div className="driftview-strip">
                    <span className="drift-sev-pill breaking">
                      BREAKING <b>{driftReport.stale_count}</b>
                    </span>
                    <span className="drift-sev-pill warning">
                      WARNING <b>{driftReport.changed_count}</b>
                    </span>
                    <span className="drift-sev-pill info">
                      INFO <b>{driftReport.new_count}</b>
                    </span>
                    <span className="driftview-meta">
                      vs baseline {"·"} {items.length} change
                      {items.length !== 1 ? "s" : ""}
                    </span>
                    <div className="tb-spacer" />
                    <button
                      className="drift-sync-main"
                      disabled={driftLoading}
                      onClick={fixDriftAll}
                    >
                      {driftLoading ? "Syncing…" : "Sync collection"}
                    </button>
                  </div>
                  <div className="driftview-cols">
                    <div className="driftview-list">
                      {items.map((item, i) => {
                        const sev = driftSeverity(item.category);
                        return (
                          <div
                            key={driftKey(item)}
                            className={`driftview-item ${sev.cls} ${i === selIdx ? "selected" : ""}`}
                            onClick={() => setSelectedDriftIdx(i)}
                          >
                            <span className={`driftview-glyph ${sev.cls}`}>
                              {sev.glyph}
                            </span>
                            <span
                              className="method-badge"
                              style={{
                                color:
                                  METHOD_COLORS[item.method] ??
                                  METHOD_COLOR_FALLBACK,
                              }}
                            >
                              {item.method}
                            </span>
                            <span className="driftview-route">{item.route}</span>
                            <span className="driftview-itemname">
                              {item.name}
                            </span>
                          </div>
                        );
                      })}
                    </div>
                    <div className="driftview-detail">
                      <div className={`driftview-detail-sev ${selSev.cls}`}>
                        {selSev.label}
                      </div>
                      <div className="driftview-detail-route">
                        <span
                          className="method-badge"
                          style={{
                            color:
                              METHOD_COLORS[sel.method] ?? METHOD_COLOR_FALLBACK,
                          }}
                        >
                          {sel.method}
                        </span>
                        <span className="driftview-detail-path">
                          {sel.route}
                        </span>
                      </div>
                      <div className="driftview-detail-name">{sel.name}</div>
                      {sel.changes && sel.changes.length > 0 && (
                        <div className="driftview-changes">
                          {sel.changes.map((c, j) => (
                            <div
                              key={j}
                              className={`driftview-change ${selSev.cls}`}
                            >
                              {c}
                            </div>
                          ))}
                        </div>
                      )}
                      {sel.request_path && (
                        <div className="driftview-affects">
                          <span className="driftview-affects-label">
                            AFFECTS
                          </span>
                          <code className="driftview-affects-file">
                            {sel.request_path}
                          </code>
                        </div>
                      )}
                      <div className="driftview-actions">
                        <button
                          className="drift-sync-main"
                          disabled={driftLoading}
                          onClick={fixDriftAll}
                        >
                          Sync collection
                        </button>
                        <button
                          className="drift-ignore-btn"
                          onClick={() => {
                            setIgnoredDrift((prev) =>
                              new Set(prev).add(driftKey(sel))
                            );
                            setSelectedDriftIdx(null);
                          }}
                        >
                          Ignore
                        </button>
                      </div>
                    </div>
                  </div>
                </>
              );
            })()
          )}
        </section>
      ) : (
        <>
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
            style={{ color: METHOD_COLORS[method] ?? METHOD_COLOR_FALLBACK }}
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
          <button className="send-btn" onClick={() => handleSend()} disabled={loading || chainLoading}>
            {loading ? "Sending..." : "Send"}
          </button>
          {chainSteps.length > 0 && (
            <button className="chain-btn" onClick={handleRunChain} disabled={loading || chainLoading}>
              {chainLoading ? "Running..." : `Run Chain (${chainSteps.length})`}
            </button>
          )}
          <button className="save-btn" onClick={handleSaveRequest}>
            Save
          </button>
          <button
            className={`secrets-toggle-btn ${secretsRevealed ? "revealed" : ""}`}
            onClick={() => setSecretsRevealed(!secretsRevealed)}
            title={secretsRevealed ? "Hide secret values" : "Reveal secret values"}
          >
            {secretsRevealed ? "\uD83D\uDD13" : "\uD83D\uDD12"}
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
              className={`tab ${activeTab === "chain" ? "active" : ""}`}
              onClick={() => setActiveTab("chain")}
            >
              Chain
              {chainDraft.length > 0 && (
                <span className="tab-count">{chainDraft.length}</span>
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
                <div className="coming-soon">
                  <span className="coming-soon-badge">Coming soon</span>
                  <p className="coming-soon-title">Auth helpers</p>
                  <p className="coming-soon-sub">
                    For now, add auth via the <strong>Headers</strong> tab (e.g.{" "}
                    <code>Authorization: Bearer {"{{token}}"}</code>) or an{" "}
                    <strong>environment</strong> variable / template.
                  </p>
                </div>
              </div>
            )}
            {activeTab === "body" && (
              <div className="body-tab">
                <div className="body-type-row">
                  <div className="body-type-chips">
                    {(["json", "text", "formdata"] as const).map((t) => (
                      <button
                        key={t}
                        className={`body-type-chip ${bodyType === t ? "active" : ""}`}
                        onClick={() => setBodyType(t)}
                      >
                        {t === "json"
                          ? "JSON"
                          : t === "text"
                            ? "Text"
                            : "Form Data"}
                      </button>
                    ))}
                  </div>
                  <span className="body-type-mime">
                    {bodyType === "json"
                      ? "application/json"
                      : bodyType === "text"
                        ? "text/plain"
                        : "x-www-form-urlencoded"}
                  </span>
                </div>
                {bodyType === "formdata" ? (
                  <div className="formdata-editor">
                    {(formDataPairs.length > 0
                      ? formDataPairs
                      : [{ key: "", value: "", enabled: true }]
                    ).map((p, i) => (
                      <div key={i} className="query-param-row">
                        <input
                          type="checkbox"
                          className="query-param-checkbox"
                          checked={p.enabled !== false}
                          onChange={(e) => {
                            const updated = [...formDataPairs];
                            if (i >= updated.length)
                              updated.push({
                                key: "",
                                value: "",
                                enabled: e.target.checked,
                              });
                            else
                              updated[i] = {
                                ...updated[i],
                                enabled: e.target.checked,
                              };
                            setFormDataPairs(updated);
                          }}
                        />
                        <input
                          className="query-param-key"
                          type="text"
                          placeholder="key"
                          value={p.key}
                          onChange={(e) => {
                            const updated =
                              formDataPairs.length > 0
                                ? [...formDataPairs]
                                : [{ key: "", value: "", enabled: true }];
                            updated[i] = { ...updated[i], key: e.target.value };
                            if (i === updated.length - 1 && e.target.value)
                              updated.push({ key: "", value: "", enabled: true });
                            setFormDataPairs(updated);
                          }}
                        />
                        <input
                          className="query-param-value"
                          type="text"
                          placeholder="value"
                          value={p.value}
                          onChange={(e) => {
                            const updated =
                              formDataPairs.length > 0
                                ? [...formDataPairs]
                                : [{ key: "", value: "", enabled: true }];
                            updated[i] = { ...updated[i], value: e.target.value };
                            setFormDataPairs(updated);
                          }}
                        />
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="body-editor-wrap">
                    <Suspense
                      fallback={
                        <textarea
                          className="editor-area"
                          value={bodyText}
                          onChange={(e) => setBodyText(e.target.value)}
                        />
                      }
                    >
                      <MonacoEditor
                        height="100%"
                        language={bodyType === "json" ? "json" : "plaintext"}
                        theme={WIRE_MONACO_THEME}
                        beforeMount={defineWireMonacoTheme}
                        value={bodyText}
                        onChange={(value) => setBodyText(value ?? "")}
                        options={{
                          minimap: { enabled: false },
                          lineNumbers: "on",
                          wordWrap: "on",
                          scrollBeyondLastLine: false,
                          fontSize: 12.5,
                          lineHeight: 1.85,
                          fontFamily:
                            '"JetBrains Mono", "Cascadia Code", "Fira Code", "Consolas", monospace',
                          padding: { top: 12, bottom: 12 },
                          renderLineHighlight: "all",
                          automaticLayout: true,
                        }}
                      />
                    </Suspense>
                  </div>
                )}
              </div>
            )}
            {activeTab === "tests" && (
              <div className="test-results-panel tests-2col">
                <div className="tests-left">
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
                                (a as unknown as Record<string, unknown>)[op] = true;
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
                      ✓ {testResults.filter((r) => r.passed).length} passed
                    </span>
                    {testResults.some((r) => !r.passed) && (
                      <span className="test-fail-count">
                        ✗ {testResults.filter((r) => !r.passed).length} failed
                      </span>
                    )}
                    {response && (
                      <span className="test-summary-time">
                        {response.elapsed_ms} ms
                      </span>
                    )}
                    <span className="test-summary-exit">
                      exit code {testResults.every((r) => r.passed) ? 0 : 1}
                    </span>
                  </div>
                )}
                </div>

                <div className="tests-right">
                  <div className="snap-head">
                    <span className="snap-dot" />
                    <span className="snap-path">
                      .wire/snapshots/
                      <span className="accent">
                        {(selectedRequestName ?? "request")
                          .replace(/\s+/g, "-")
                          .toLowerCase()}
                        .json
                      </span>
                    </span>
                    {response && selectedRequestPath && (
                      <button className="snap-save-btn" onClick={handleSaveSnapshot}>
                        {snapshotComparison?.exists
                          ? "Update snapshot"
                          : "Save snapshot"}
                      </button>
                    )}
                  </div>
                  {!selectedRequestPath ? (
                    <div className="snap-empty">
                      Save this request to a collection to capture response
                      snapshots and catch regressions.
                    </div>
                  ) : !response ? (
                    <div className="snap-empty">
                      Send the request, then save a snapshot to lock in the
                      expected response. Future sends diff against it.
                    </div>
                  ) : !snapshotComparison || !snapshotComparison.exists ? (
                    <div className="snap-empty">
                      No snapshot saved yet.{" "}
                      <button
                        className="snap-inline-link"
                        onClick={handleSaveSnapshot}
                      >
                        Save the current response
                      </button>{" "}
                      as the baseline.
                    </div>
                  ) : snapshotComparison.entries.length === 0 &&
                    snapshotComparison.status_old ===
                      snapshotComparison.status_new ? (
                    <div className="snap-match">✓ Matches snapshot</div>
                  ) : (
                    <>
                      <div className="snap-diff">
                        {snapshotComparison.status_old !==
                          snapshotComparison.status_new && (
                          <div className="snap-line changed">
                            <span className="snap-glyph">~</span>
                            <span className="snap-field">status</span>
                            <span className="snap-vals">
                              {snapshotComparison.status_old} →{" "}
                              {snapshotComparison.status_new}
                            </span>
                          </div>
                        )}
                        {snapshotComparison.entries.map((e, i) => {
                          const cls =
                            e.kind === "Added"
                              ? "added"
                              : e.kind === "Removed"
                                ? "removed"
                                : "changed";
                          const glyph =
                            e.kind === "Added"
                              ? "+"
                              : e.kind === "Removed"
                                ? "−"
                                : "~";
                          return (
                            <div key={i} className={`snap-line ${cls}`}>
                              <span className="snap-glyph">{glyph}</span>
                              <span className="snap-field">
                                {e.path || "(root)"}
                              </span>
                              <span className="snap-vals">
                                {diffValueText(e)}
                              </span>
                            </div>
                          );
                        })}
                      </div>
                      <div className="snap-foot">
                        <span className="snap-foot-added">
                          +
                          {
                            snapshotComparison.entries.filter(
                              (e) => e.kind === "Added"
                            ).length
                          }
                        </span>
                        <span className="snap-foot-removed">
                          −
                          {
                            snapshotComparison.entries.filter(
                              (e) => e.kind === "Removed"
                            ).length
                          }
                        </span>
                        <span className="snap-foot-changed">
                          ~
                          {
                            snapshotComparison.entries.filter(
                              (e) => e.kind === "Changed"
                            ).length
                          }
                        </span>
                        <span className="snap-foot-label">vs snapshot</span>
                      </div>
                    </>
                  )}
                </div>
              </div>
            )}
            {activeTab === "chain" && (
              <div className="chain-builder">
                <div className="chain-builder-head">
                  <h3 className="chain-builder-title">Chain steps</h3>
                  <span className="chain-builder-hint">
                    Each step runs a saved request; extract values to reuse as{" "}
                    {"{{var}}"} in later steps.
                  </span>
                </div>
                {(() => {
                  const prefix = activeCollection
                    ? activeCollection.path + "/requests/"
                    : "";
                  const runOptions = (activeCollection?.info.requests ?? []).map(
                    (r) => {
                      const rel = r.path.startsWith(prefix)
                        ? r.path.slice(prefix.length).replace(/\.wire\.yaml$/, "")
                        : r.name;
                      return rel;
                    }
                  );
                  const set = (d: ChainDraftStep[]) => updateChainDraft(d);
                  return (
                    <>
                      {chainDraft.length === 0 && (
                        <p className="placeholder">
                          No steps yet — add a request to build a chain.
                        </p>
                      )}
                      {chainDraft.map((step, i) => (
                        <div key={i} className="chainb-card">
                          <div className="chainb-head">
                            <span className="chainb-num">{i + 1}</span>
                            <select
                              className="chainb-run"
                              value={step.run}
                              onChange={(e) => {
                                const d = [...chainDraft];
                                d[i] = { ...d[i], run: e.target.value };
                                set(d);
                              }}
                            >
                              <option value="">Select request…</option>
                              {runOptions.map((o) => (
                                <option key={o} value={o}>
                                  {o}
                                </option>
                              ))}
                            </select>
                            <button
                              className="chainb-icon"
                              disabled={i === 0}
                              title="Move up"
                              onClick={() => {
                                const d = [...chainDraft];
                                [d[i - 1], d[i]] = [d[i], d[i - 1]];
                                set(d);
                              }}
                            >
                              ↑
                            </button>
                            <button
                              className="chainb-icon"
                              disabled={i === chainDraft.length - 1}
                              title="Move down"
                              onClick={() => {
                                const d = [...chainDraft];
                                [d[i + 1], d[i]] = [d[i], d[i + 1]];
                                set(d);
                              }}
                            >
                              ↓
                            </button>
                            <button
                              className="chainb-icon chainb-del"
                              title="Remove step"
                              onClick={() =>
                                set(chainDraft.filter((_, j) => j !== i))
                              }
                            >
                              ✕
                            </button>
                          </div>
                          <div className="chainb-extracts">
                            <span className="chainb-extract-label">EXTRACTS</span>
                            {step.extracts.map((ex, j) => (
                              <div key={j} className="chainb-extract-row">
                                <input
                                  className="chainb-extract-name"
                                  placeholder="var"
                                  value={ex.name}
                                  onChange={(e) => {
                                    const d = [...chainDraft];
                                    const ex2 = [...d[i].extracts];
                                    ex2[j] = { ...ex2[j], name: e.target.value };
                                    d[i] = { ...d[i], extracts: ex2 };
                                    set(d);
                                  }}
                                />
                                <span className="chainb-arrow">←</span>
                                <input
                                  className="chainb-extract-path"
                                  placeholder="body.token"
                                  value={ex.path}
                                  onChange={(e) => {
                                    const d = [...chainDraft];
                                    const ex2 = [...d[i].extracts];
                                    ex2[j] = { ...ex2[j], path: e.target.value };
                                    d[i] = { ...d[i], extracts: ex2 };
                                    set(d);
                                  }}
                                />
                                <button
                                  className="chainb-icon chainb-del"
                                  title="Remove extract"
                                  onClick={() => {
                                    const d = [...chainDraft];
                                    d[i] = {
                                      ...d[i],
                                      extracts: d[i].extracts.filter(
                                        (_, k) => k !== j
                                      ),
                                    };
                                    set(d);
                                  }}
                                >
                                  ✕
                                </button>
                              </div>
                            ))}
                            <button
                              className="chainb-add-extract"
                              onClick={() => {
                                const d = [...chainDraft];
                                d[i] = {
                                  ...d[i],
                                  extracts: [
                                    ...d[i].extracts,
                                    { name: "", path: "" },
                                  ],
                                };
                                set(d);
                              }}
                            >
                              + extract
                            </button>
                          </div>
                          <label className="chainb-persist">
                            <input
                              type="checkbox"
                              checked={step.persist}
                              onChange={(e) => {
                                const d = [...chainDraft];
                                d[i] = { ...d[i], persist: e.target.checked };
                                set(d);
                              }}
                            />
                            Persist extracted vars to the environment
                          </label>
                        </div>
                      ))}
                      <button
                        className="chainb-add-step"
                        onClick={() =>
                          set([
                            ...chainDraft,
                            { run: "", extracts: [], persist: false },
                          ])
                        }
                      >
                        + Add step
                      </button>
                      {chainDraft.length > 0 && (
                        <p className="chainb-save-hint">
                          Press <strong>Save</strong> to write the chain, then{" "}
                          <strong>Run Chain</strong> to execute it.
                        </p>
                      )}
                    </>
                  );
                })()}
              </div>
            )}
            {activeTab === "pre-run" && (
              <div className="tab-placeholder">
                <div className="coming-soon">
                  <span className="coming-soon-badge">Coming soon</span>
                  <p className="coming-soon-title">Pre-request scripts</p>
                  <p className="coming-soon-sub">
                    Scriptable pre-request hooks aren't part of Wire yet. Use{" "}
                    <strong>chains</strong> to feed one request's output into the
                    next, or environment variables for setup.
                  </p>
                </div>
              </div>
            )}
          </div>
        </div>
      </main>

      {/* Right Panel: Response Viewer */}
      <section className="response-viewer">
        <div className="response-header">
          {response ? (
            <>
              <span
                className="status-pill"
                style={{ color: statusColor(response.status) }}
              >
                <span
                  className="status-dot"
                  style={{ background: statusColor(response.status) }}
                />
                {response.status} {response.status_text}
              </span>
              <span className="response-metric">{response.elapsed_ms} ms</span>
              <span className="response-metric-sep">·</span>
              <span className="response-metric">
                {formatBytes(response.size_bytes)}
              </span>
              <div className="tb-spacer" />
              {snapshotMsg && (
                <span className="response-snap-msg">{snapshotMsg}</span>
              )}
              {selectedRequestPath && (
                <button
                  className="response-action"
                  title="Save this response as the golden-file snapshot"
                  onClick={handleSaveSnapshot}
                >
                  Snapshot
                </button>
              )}
              <button
                className="response-action"
                title="Copy response body"
                onClick={() => navigator.clipboard?.writeText(response.body)}
              >
                Copy
              </button>
            </>
          ) : (
            <span className="response-header-title">Response</span>
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
                <span className="tab-count">
                  {Object.keys(response.headers).length}
                </span>
              </button>
              {testResults.length > 0 && (
                <button
                  className={`tab ${responseTab === "tests" ? "active" : ""}`}
                  onClick={() => setResponseTab("tests")}
                >
                  Tests
                  <span
                    className={`tab-badge ${testResults.every((r) => r.passed) ? "tab-badge-pass" : "tab-badge-fail"}`}
                  >
                    {testResults.filter((r) => r.passed).length}/
                    {testResults.length}
                  </span>
                </button>
              )}
            </div>
            {responseTab === "body" ? (
              <div className="response-body-wrap">
                <Suspense
                  fallback={
                    <pre className="response-body">
                      {formatBody(response.body)}
                    </pre>
                  }
                >
                  <MonacoEditor
                    height="100%"
                    defaultLanguage="json"
                    theme={WIRE_MONACO_THEME}
                    beforeMount={defineWireMonacoTheme}
                    value={formatBody(response.body)}
                    options={{
                      readOnly: true,
                      domReadOnly: true,
                      minimap: { enabled: false },
                      lineNumbers: "on",
                      wordWrap: "on",
                      scrollBeyondLastLine: false,
                      fontSize: 12.5,
                      lineHeight: 1.85,
                      fontFamily:
                        '"JetBrains Mono", "Cascadia Code", "Fira Code", monospace',
                      padding: { top: 12, bottom: 12 },
                      automaticLayout: true,
                    }}
                  />
                </Suspense>
              </div>
            ) : responseTab === "headers" ? (
              <div className="panel-body">
                <div className="response-headers">
                  {Object.entries(response.headers).map(([key, value]) => (
                    <div key={key} className="header-row">
                      <span className="header-key">{key}</span>
                      <span className="header-value">{value}</span>
                    </div>
                  ))}
                </div>
              </div>
            ) : (
              <div className="panel-body">
                <div className="response-tests">
                  {testResults.map((r, i) => (
                    <div
                      key={i}
                      className={`response-test-row ${r.passed ? "passed" : "failed"}`}
                    >
                      <span className="test-result-icon">
                        {r.passed ? "✓" : "✗"}
                      </span>
                      <span className="response-test-field">{r.field}</span>
                      <span className="response-test-op">{r.operator}</span>
                      <span className="response-test-exp">{r.expected}</span>
                      {!r.passed && (
                        <span className="test-result-actual">
                          got {r.actual}
                        </span>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}
            {testResults.length > 0 && (
              <div
                className={`response-pass-bar ${testResults.every((r) => r.passed) ? "ok" : "fail"}`}
              >
                <span className="pass-bar-icon">
                  {testResults.every((r) => r.passed) ? "✓" : "✗"}
                </span>
                <span className="pass-bar-count">
                  {testResults.filter((r) => r.passed).length} /{" "}
                  {testResults.length} passed
                </span>
                <span className="pass-bar-fields">
                  {testResults
                    .map((r) => r.field)
                    .slice(0, 6)
                    .join("  ·  ")}
                </span>
              </div>
            )}
          </>
        )}

        {!response && !error && !chainResult && chainSteps.length === 0 && (
          <div className="panel-body">
            <p className="placeholder">Send a request to see the response</p>
          </div>
        )}

        {chainSteps.length > 0 && !chainResult && !chainLoading && (
          <div className="wirechain">
            <div className="wirechain-head">
              <span className="wirechain-title">
                Chain {"\u00b7"} {chainSteps.length} step
                {chainSteps.length !== 1 ? "s" : ""}
              </span>
              <div className="tb-spacer" />
              <button
                className="wirechain-run"
                onClick={handleRunChain}
                disabled={loading || chainLoading}
              >
                Run Chain
              </button>
            </div>
            <div className="wirechain-flow">
              {chainSteps.map((step, i) => {
                const last = i === chainSteps.length - 1;
                const name =
                  step.run.split("/").pop()?.replace(/\.wire\.yaml$/, "") ??
                  step.run;
                return (
                  <div key={i} className="wirechain-row">
                    <div className="wirechain-rail">
                      <span className="wirechain-node">{i + 1}</span>
                      {!last && <span className="wirechain-line" />}
                    </div>
                    <div className="wirechain-card">
                      <div className="wirechain-card-head">
                        <span className="wirechain-name">{name}</span>
                        <span className="wirechain-path">{step.run}</span>
                      </div>
                      {step.extract && Object.keys(step.extract).length > 0 && (
                        <div className="wirechain-band">
                          <span className="wirechain-band-label">EXTRACTS</span>
                          {Object.entries(step.extract).map(([k, v]) => (
                            <span key={k} className="wirechain-chip extract">
                              {k} = {v}
                            </span>
                          ))}
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {chainResult && (
          <div className="wirechain">
            <div className="wirechain-head">
              <span
                className={`wirechain-status ${chainResult.success ? "ok" : "fail"}`}
              >
                {chainResult.success ? "✓" : "✗"}
              </span>
              <span className="wirechain-title">
                {chainResult.steps.length} step
                {chainResult.steps.length !== 1 ? "s" : ""} {"·"}{" "}
                {chainResult.success ? "all passed" : "failed"} {"·"}{" "}
                {chainResult.total_elapsed_ms} ms
              </span>
              <div className="tb-spacer" />
              <button
                className="wirechain-run"
                onClick={handleRunChain}
                disabled={loading || chainLoading}
              >
                {chainLoading ? "Running…" : "Run Chain"}
              </button>
            </div>
            <div className="wirechain-flow">
              {chainResult.steps.map((step, idx) => {
                const last = idx === chainResult.steps.length - 1;
                const isExpanded = expandedChainSteps.has(step.step_index);
                return (
                  <div
                    key={step.step_index}
                    className={`wirechain-row ${step.passed ? "pass" : "fail"}`}
                  >
                    <div className="wirechain-rail">
                      <span
                        className={`wirechain-node ${step.passed ? "pass" : "fail"} ${last && step.passed ? "final" : ""}`}
                      >
                        {last && step.passed ? "✓" : step.step_index + 1}
                      </span>
                      {!last && <span className="wirechain-line" />}
                    </div>
                    <div className="wirechain-card">
                      <div
                        className="wirechain-card-head clickable"
                        onClick={() =>
                          setExpandedChainSteps((prev) => {
                            const next = new Set(prev);
                            if (next.has(step.step_index))
                              next.delete(step.step_index);
                            else next.add(step.step_index);
                            return next;
                          })
                        }
                      >
                        <span
                          className="method-badge"
                          style={{
                            color:
                              METHOD_COLORS[step.request_method] ??
                              METHOD_COLOR_FALLBACK,
                          }}
                        >
                          {step.request_method}
                        </span>
                        <span className="wirechain-name">
                          {step.request_name}
                        </span>
                        <span className="wirechain-path">
                          {step.request_url}
                        </span>
                        <div className="tb-spacer" />
                        {step.status > 0 && (
                          <span
                            className="status-pill"
                            style={{ color: statusColor(step.status) }}
                          >
                            <span
                              className="status-dot"
                              style={{ background: statusColor(step.status) }}
                            />
                            {step.status}
                          </span>
                        )}
                        <span className="wirechain-time">
                          {step.elapsed_ms} ms
                        </span>
                        <span className="wirechain-toggle">
                          {isExpanded ? "▾" : "▸"}
                        </span>
                      </div>
                      {Object.keys(step.extracted).length > 0 && (
                        <div className="wirechain-band">
                          <span className="wirechain-band-label">EXTRACTS</span>
                          {Object.entries(step.extracted).map(([name, val]) => (
                            <span
                              key={name}
                              className="wirechain-chip extract"
                              title={val}
                            >
                              {name} ={" "}
                              {val.length > 40 ? val.slice(0, 37) + "…" : val}
                            </span>
                          ))}
                        </div>
                      )}
                      {step.error && (
                        <div className="wirechain-error">{step.error}</div>
                      )}
                      {isExpanded && (
                        <div className="chain-step-detail">
                          <div className="chain-detail-section">
                            <div className="chain-detail-label">Request</div>
                            {Object.keys(step.request_headers).length > 0 && (
                              <div className="chain-detail-headers">
                                {Object.entries(step.request_headers).map(
                                  ([k, v]) => (
                                    <div key={k} className="chain-detail-header">
                                      <span className="header-key">{k}:</span>{" "}
                                      <span className="header-value">{v}</span>
                                    </div>
                                  )
                                )}
                              </div>
                            )}
                          </div>
                          <div className="chain-detail-section">
                            <div className="chain-detail-label">
                              Response{" "}
                              {step.status > 0 && (
                                <span style={{ color: statusColor(step.status) }}>
                                  {step.status} {step.status_text}
                                </span>
                              )}
                            </div>
                            {step.response_body && (
                              <pre className="chain-detail-body">
                                {(() => {
                                  try {
                                    return JSON.stringify(
                                      JSON.parse(step.response_body),
                                      null,
                                      2
                                    );
                                  } catch {
                                    return step.response_body;
                                  }
                                })()}
                              </pre>
                            )}
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
              {chainResult.error && !chainResult.success && (
                <div className="chain-error">{chainResult.error}</div>
              )}
            </div>
          </div>
        )}
      </section>
        </>
      )}
      </div>
      )}

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

      {confirmDelete && (
        <div className="prompt-backdrop" onClick={() => setConfirmDelete(null)}>
          <div className="prompt-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="prompt-title">
              Delete request “{confirmDelete.name}”?
            </div>
            <p className="confirm-sub">
              This removes the <code>.wire.yaml</code> file from disk and can't
              be undone.
            </p>
            <div className="prompt-actions">
              <button
                className="prompt-btn prompt-cancel"
                onClick={() => setConfirmDelete(null)}
              >
                Cancel
              </button>
              <button
                className="prompt-btn prompt-delete"
                onClick={() => {
                  handleDeleteRequest(confirmDelete.path);
                  setConfirmDelete(null);
                }}
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
