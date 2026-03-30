import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { useState } from "react";
import type { IpcCollectionInfo, IpcRequestEntry } from "./types";
import type { TreeNode } from "./utils";
import { buildTree, filterTree, METHOD_COLORS } from "./utils";

/**
 * Since App.tsx is a monolith with Tauri IPC, we test the collection sidebar
 * rendering logic by extracting the key patterns into test-local components
 * that mirror the actual App.tsx implementation.
 */

// Mirrors the TreeItem component from App.tsx
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
        data-testid={`request-${node.entry.name}`}
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

// Test harness mirroring the collection accordion from App.tsx
function CollectionAccordionTest({
  collections: initialCollections,
  onSelect,
  onDelete,
  onRename,
  onAddRequest,
  filterText = "",
}: {
  collections: Array<{ info: IpcCollectionInfo; path: string }>;
  onSelect: (path: string, entry: IpcRequestEntry) => void;
  onDelete?: (path: string) => void;
  onRename?: (path: string, currentName: string) => void;
  onAddRequest?: (path: string) => void;
  filterText?: string;
}) {
  const [collections, setCollections] = useState(initialCollections);
  const [expandedCollections, setExpandedCollections] = useState<Set<string>>(
    new Set()
  );

  const toggleCollection = (path: string) => {
    setExpandedCollections((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const handleDelete = (path: string) => {
    setCollections((prev) => prev.filter((c) => c.path !== path));
    onDelete?.(path);
  };

  return (
    <div className="sidebar-tree">
      {collections.length === 0 && (
        <div className="empty-state">
          <p className="empty-state-title">No Collections Available</p>
        </div>
      )}
      {collections.map(({ info, path }) => {
        const isExpanded = expandedCollections.has(path);
        const tree = buildTree(info.requests, path);
        const filtered = filterTree(tree, filterText);
        const sorted = [...filtered.children.values()].sort((a, b) => {
          const aIsFolder = !a.entry;
          const bIsFolder = !b.entry;
          if (aIsFolder !== bIsFolder) return aIsFolder ? -1 : 1;
          return a.name.localeCompare(b.name);
        });
        return (
          <div key={path} className="collection-accordion">
            <div
              className="collection-header"
              data-testid={`collection-${info.name}`}
              onClick={() => toggleCollection(path)}
            >
              <span className="folder-icon">
                {isExpanded ? "\u25BE" : "\u25B8"}
              </span>
              <span className="collection-name">{info.name}</span>
              <span className="collection-count">{info.requests.length}</span>
              <span className="collection-actions">
                <button
                  className="collection-action-btn"
                  title="Add request"
                  data-testid={`add-request-${info.name}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    onAddRequest?.(path);
                  }}
                >
                  +
                </button>
                <button
                  className="collection-action-btn"
                  title="Rename collection"
                  data-testid={`rename-${info.name}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    onRename?.(path, info.name);
                  }}
                >
                  &#x270E;
                </button>
                <button
                  className="collection-action-btn collection-action-delete"
                  title="Remove collection"
                  data-testid={`delete-${info.name}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleDelete(path);
                  }}
                >
                  &#x2715;
                </button>
              </span>
            </div>
            {isExpanded && (
              <div className="collection-requests">
                {sorted.length === 0 && (
                  <p className="placeholder">No requests yet</p>
                )}
                {sorted.map((child) => (
                  <TreeItem
                    key={child.entry?.path ?? child.name}
                    node={child}
                    depth={1}
                    onSelect={(entry) => onSelect(path, entry)}
                    selectedPath={null}
                  />
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

// Test harness for dropdown menu
function DropdownTest({
  onNew,
  onImport,
  onImportUrl,
}: {
  onNew: () => void;
  onImport: () => void;
  onImportUrl: () => void;
}) {
  const [dropdownOpen, setDropdownOpen] = useState(false);
  return (
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
            data-testid="dropdown-backdrop"
            onClick={() => setDropdownOpen(false)}
          />
          <div className="dropdown-menu">
            <button
              className="dropdown-item"
              onClick={() => {
                setDropdownOpen(false);
                onNew();
              }}
            >
              New Collection
            </button>
            <button
              className="dropdown-item"
              onClick={() => {
                setDropdownOpen(false);
                onImport();
              }}
            >
              Import Collection
            </button>
            <button
              className="dropdown-item"
              onClick={() => {
                setDropdownOpen(false);
                onImportUrl();
              }}
            >
              Import from URL
            </button>
          </div>
        </>
      )}
    </div>
  );
}

// --- Test data ---

const makeCollection = (
  name: string,
  path: string,
  requests: IpcRequestEntry[] = []
): { info: IpcCollectionInfo; path: string } => ({
  info: {
    name,
    version: 1,
    active_env: null,
    default_templates: [],
    requests,
    environments: [],
    templates: [],
    source_dir: null,
  },
  path,
});

const basePath1 = "/projects/api/.wire";
const basePath2 = "/projects/web/.wire";

const apiRequests: IpcRequestEntry[] = [
  { path: `${basePath1}/requests/get-users.wire.yaml`, name: "Get Users", method: "GET" },
  { path: `${basePath1}/requests/create-user.wire.yaml`, name: "Create User", method: "POST" },
  { path: `${basePath1}/requests/delete-user.wire.yaml`, name: "Delete User", method: "DELETE" },
];

const webRequests: IpcRequestEntry[] = [
  { path: `${basePath2}/requests/login.wire.yaml`, name: "Login", method: "POST" },
  { path: `${basePath2}/requests/health.wire.yaml`, name: "Health Check", method: "GET" },
];

// --- Tests ---

describe("Dropdown Menu", () => {
  it("is initially closed (no menu items visible)", () => {
    render(
      <DropdownTest onNew={vi.fn()} onImport={vi.fn()} onImportUrl={vi.fn()} />
    );
    expect(screen.queryByText("New Collection")).toBeNull();
    expect(screen.queryByText("Import Collection")).toBeNull();
    expect(screen.queryByText("Import from URL")).toBeNull();
  });

  it("opens when trigger button is clicked", () => {
    render(
      <DropdownTest onNew={vi.fn()} onImport={vi.fn()} onImportUrl={vi.fn()} />
    );
    fireEvent.click(screen.getByText(/Collections/));
    expect(screen.getByText("New Collection")).toBeDefined();
    expect(screen.getByText("Import Collection")).toBeDefined();
    expect(screen.getByText("Import from URL")).toBeDefined();
  });

  it("closes when backdrop is clicked", () => {
    render(
      <DropdownTest onNew={vi.fn()} onImport={vi.fn()} onImportUrl={vi.fn()} />
    );
    fireEvent.click(screen.getByText(/Collections/));
    expect(screen.getByText("New Collection")).toBeDefined();

    fireEvent.click(screen.getByTestId("dropdown-backdrop"));
    expect(screen.queryByText("New Collection")).toBeNull();
  });

  it("calls onNew and closes when New Collection is clicked", () => {
    const onNew = vi.fn();
    render(
      <DropdownTest onNew={onNew} onImport={vi.fn()} onImportUrl={vi.fn()} />
    );
    fireEvent.click(screen.getByText(/Collections/));
    fireEvent.click(screen.getByText("New Collection"));
    expect(onNew).toHaveBeenCalledTimes(1);
    // Menu should be closed now
    expect(screen.queryByText("Import Collection")).toBeNull();
  });

  it("calls onImport and closes when Import Collection is clicked", () => {
    const onImport = vi.fn();
    render(
      <DropdownTest onNew={vi.fn()} onImport={onImport} onImportUrl={vi.fn()} />
    );
    fireEvent.click(screen.getByText(/Collections/));
    fireEvent.click(screen.getByText("Import Collection"));
    expect(onImport).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("New Collection")).toBeNull();
  });

  it("calls onImportUrl and closes when Import from URL is clicked", () => {
    const onImportUrl = vi.fn();
    render(
      <DropdownTest onNew={vi.fn()} onImport={vi.fn()} onImportUrl={onImportUrl} />
    );
    fireEvent.click(screen.getByText(/Collections/));
    fireEvent.click(screen.getByText("Import from URL"));
    expect(onImportUrl).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("New Collection")).toBeNull();
  });
});

describe("Collection Accordion", () => {
  it("shows empty state when no collections exist", () => {
    render(<CollectionAccordionTest collections={[]} onSelect={vi.fn()} />);
    expect(screen.getByText("No Collections Available")).toBeDefined();
  });

  it("renders each collection as an accordion header", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
      makeCollection("Web Collection", basePath2, webRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );
    expect(screen.getByText("API Collection")).toBeDefined();
    expect(screen.getByText("Web Collection")).toBeDefined();
  });

  it("shows request count in accordion header", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );
    // apiRequests has 3 items
    expect(screen.getByText("3")).toBeDefined();
  });

  it("does not show requests when accordion is collapsed", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );
    // Accordion starts collapsed
    expect(screen.queryByText("Get Users")).toBeNull();
    expect(screen.queryByText("Create User")).toBeNull();
  });

  it("shows requests when accordion header is clicked", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );
    fireEvent.click(screen.getByTestId("collection-API Collection"));
    expect(screen.getByText("Get Users")).toBeDefined();
    expect(screen.getByText("Create User")).toBeDefined();
    expect(screen.getByText("Delete User")).toBeDefined();
  });

  it("shows color-coded method badges for endpoints", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );
    fireEvent.click(screen.getByTestId("collection-API Collection"));

    // Check method badges exist with correct text
    const badges = screen.getAllByText(/^(GET|POST|DELETE)$/);
    expect(badges.length).toBe(3);

    // Verify GET badge has correct color (jsdom converts hex #4ec9b0 to rgb)
    const getBadge = screen.getByTestId("request-Get Users").querySelector(".method-badge");
    expect(getBadge).toBeDefined();
    expect(getBadge!.textContent).toBe("GET");
    expect(getBadge!.getAttribute("style")).toContain("rgb(78, 201, 176)");

    // Verify POST badge (#dcdcaa)
    const postBadge = screen.getByTestId("request-Create User").querySelector(".method-badge");
    expect(postBadge!.textContent).toBe("POST");
    expect(postBadge!.getAttribute("style")).toContain("rgb(220, 220, 170)");

    // Verify DELETE badge (#f44747)
    const deleteBadge = screen.getByTestId("request-Delete User").querySelector(".method-badge");
    expect(deleteBadge!.textContent).toBe("DELETE");
    expect(deleteBadge!.getAttribute("style")).toContain("rgb(244, 71, 71)");
  });

  it("collapses accordion when header is clicked again", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );
    const header = screen.getByTestId("collection-API Collection");

    // Expand
    fireEvent.click(header);
    expect(screen.getByText("Get Users")).toBeDefined();

    // Collapse
    fireEvent.click(header);
    expect(screen.queryByText("Get Users")).toBeNull();
  });

  it("can have multiple collections expanded independently", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
      makeCollection("Web Collection", basePath2, webRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );

    // Expand both
    fireEvent.click(screen.getByTestId("collection-API Collection"));
    fireEvent.click(screen.getByTestId("collection-Web Collection"));

    // Both show requests
    expect(screen.getByText("Get Users")).toBeDefined();
    expect(screen.getByText("Login")).toBeDefined();

    // Collapse only API
    fireEvent.click(screen.getByTestId("collection-API Collection"));
    expect(screen.queryByText("Get Users")).toBeNull();
    expect(screen.getByText("Login")).toBeDefined(); // Web still open
  });

  it("calls onSelect with collection path and entry when endpoint clicked", () => {
    const onSelect = vi.fn();
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={onSelect} />
    );
    fireEvent.click(screen.getByTestId("collection-API Collection"));
    fireEvent.click(screen.getByTestId("request-Get Users"));

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith(basePath1, apiRequests[0]);
  });

  it("filters requests across collections", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
      makeCollection("Web Collection", basePath2, webRequests),
    ];
    render(
      <CollectionAccordionTest
        collections={collections}
        onSelect={vi.fn()}
        filterText="login"
      />
    );

    // Expand both
    fireEvent.click(screen.getByTestId("collection-API Collection"));
    fireEvent.click(screen.getByTestId("collection-Web Collection"));

    // Only Login should match
    expect(screen.queryByText("Get Users")).toBeNull();
    expect(screen.getByText("Login")).toBeDefined();
  });

  it("shows empty collection with 'No requests yet' message", () => {
    const collections = [makeCollection("Empty Collection", basePath1, [])];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );
    fireEvent.click(screen.getByTestId("collection-Empty Collection"));
    expect(screen.getByText("No requests yet")).toBeDefined();
  });
});

describe("Collection Action Buttons", () => {
  it("renders add, rename, and delete buttons on each collection header", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );
    expect(screen.getByTestId("add-request-API Collection")).toBeDefined();
    expect(screen.getByTestId("rename-API Collection")).toBeDefined();
    expect(screen.getByTestId("delete-API Collection")).toBeDefined();
  });

  it("delete button removes collection from sidebar without affecting others", () => {
    const onDelete = vi.fn();
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
      makeCollection("Web Collection", basePath2, webRequests),
    ];
    render(
      <CollectionAccordionTest
        collections={collections}
        onSelect={vi.fn()}
        onDelete={onDelete}
      />
    );

    fireEvent.click(screen.getByTestId("delete-API Collection"));

    // API collection removed
    expect(screen.queryByText("API Collection")).toBeNull();
    // Web collection still present
    expect(screen.getByText("Web Collection")).toBeDefined();
    // Callback fired with correct path
    expect(onDelete).toHaveBeenCalledWith(basePath1);
  });

  it("delete button does not toggle accordion (stopPropagation)", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );

    // Accordion starts collapsed — no requests visible
    expect(screen.queryByText("Get Users")).toBeNull();

    // Click delete button
    fireEvent.click(screen.getByTestId("delete-API Collection"));

    // Collection should be removed, NOT expanded
    expect(screen.queryByText("API Collection")).toBeNull();
    expect(screen.queryByText("Get Users")).toBeNull();
  });

  it("rename button calls onRename with path and current name without toggling accordion", () => {
    const onRename = vi.fn();
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest
        collections={collections}
        onSelect={vi.fn()}
        onRename={onRename}
      />
    );

    // Accordion starts collapsed
    expect(screen.queryByText("Get Users")).toBeNull();

    fireEvent.click(screen.getByTestId("rename-API Collection"));

    expect(onRename).toHaveBeenCalledWith(basePath1, "API Collection");
    // Accordion should NOT have expanded (stopPropagation)
    expect(screen.queryByText("Get Users")).toBeNull();
  });

  it("add request button calls onAddRequest with collection path without toggling accordion", () => {
    const onAddRequest = vi.fn();
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest
        collections={collections}
        onSelect={vi.fn()}
        onAddRequest={onAddRequest}
      />
    );

    // Accordion starts collapsed
    expect(screen.queryByText("Get Users")).toBeNull();

    fireEvent.click(screen.getByTestId("add-request-API Collection"));

    expect(onAddRequest).toHaveBeenCalledWith(basePath1);
    // Accordion should NOT have expanded
    expect(screen.queryByText("Get Users")).toBeNull();
  });

  it("action buttons have correct titles for accessibility", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );
    expect(screen.getByTitle("Add request")).toBeDefined();
    expect(screen.getByTitle("Rename collection")).toBeDefined();
    expect(screen.getByTitle("Remove collection")).toBeDefined();
  });

  it("deleting last collection shows empty state", () => {
    const collections = [
      makeCollection("API Collection", basePath1, apiRequests),
    ];
    render(
      <CollectionAccordionTest collections={collections} onSelect={vi.fn()} />
    );

    fireEvent.click(screen.getByTestId("delete-API Collection"));

    expect(screen.getByText("No Collections Available")).toBeDefined();
  });
});
