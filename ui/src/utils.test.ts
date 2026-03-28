import { describe, it, expect, vi, afterEach } from "vitest";
import {
  buildTree,
  filterTree,
  formatTimeAgo,
  METHOD_COLORS,
  statusColor,
  formatBody,
} from "./utils";
import type { IpcRequestEntry } from "./types";

describe("buildTree", () => {
  const basePath = "/home/user/project/.wire";

  it("creates leaf nodes for requests in root", () => {
    const requests: IpcRequestEntry[] = [
      {
        path: `${basePath}/requests/health.wire.yaml`,
        name: "Health Check",
        method: "GET",
      },
    ];
    const tree = buildTree(requests, basePath);
    const child = tree.children.get("health.wire.yaml");
    expect(child).toBeDefined();
    expect(child!.name).toBe("Health Check");
    expect(child!.entry?.method).toBe("GET");
  });

  it("creates folder nodes for nested paths", () => {
    const requests: IpcRequestEntry[] = [
      {
        path: `${basePath}/requests/auth/login.wire.yaml`,
        name: "Login",
        method: "POST",
      },
    ];
    const tree = buildTree(requests, basePath);
    const authFolder = tree.children.get("auth");
    expect(authFolder).toBeDefined();
    expect(authFolder!.entry).toBeUndefined();
    expect(authFolder!.children.size).toBe(1);

    const login = authFolder!.children.get("login.wire.yaml");
    expect(login!.name).toBe("Login");
  });

  it("handles deeply nested paths", () => {
    const requests: IpcRequestEntry[] = [
      {
        path: `${basePath}/requests/api/v2/admin/users.wire.yaml`,
        name: "Admin Users",
        method: "GET",
      },
    ];
    const tree = buildTree(requests, basePath);
    const api = tree.children.get("api");
    const v2 = api!.children.get("v2");
    const admin = v2!.children.get("admin");
    const users = admin!.children.get("users.wire.yaml");
    expect(users!.name).toBe("Admin Users");
  });

  it("groups sibling requests under same folder", () => {
    const requests: IpcRequestEntry[] = [
      {
        path: `${basePath}/requests/users/list.wire.yaml`,
        name: "List Users",
        method: "GET",
      },
      {
        path: `${basePath}/requests/users/create.wire.yaml`,
        name: "Create User",
        method: "POST",
      },
    ];
    const tree = buildTree(requests, basePath);
    const usersFolder = tree.children.get("users");
    expect(usersFolder!.children.size).toBe(2);
  });

  it("returns empty tree for empty request list", () => {
    const tree = buildTree([], basePath);
    expect(tree.children.size).toBe(0);
  });

  it("handles paths not matching basePath prefix", () => {
    const requests: IpcRequestEntry[] = [
      {
        path: "/other/path/req.wire.yaml",
        name: "Other",
        method: "GET",
      },
    ];
    const tree = buildTree(requests, basePath);
    // Falls through without stripping prefix — uses full path segments
    expect(tree.children.size).toBeGreaterThan(0);
  });
});

describe("filterTree", () => {
  const basePath = "/home/user/project/.wire";

  function makeTree(requests: IpcRequestEntry[]) {
    return buildTree(requests, basePath);
  }

  const requests: IpcRequestEntry[] = [
    { path: `${basePath}/requests/auth/login.wire.yaml`, name: "Login", method: "POST" },
    { path: `${basePath}/requests/auth/signup.wire.yaml`, name: "Signup", method: "POST" },
    { path: `${basePath}/requests/users/list.wire.yaml`, name: "List Users", method: "GET" },
    { path: `${basePath}/requests/health.wire.yaml`, name: "Health Check", method: "GET" },
  ];

  it("returns full tree when filter is empty", () => {
    const tree = makeTree(requests);
    const filtered = filterTree(tree, "");
    expect(filtered.children.size).toBe(tree.children.size);
  });

  it("filters leaf nodes by name (case-insensitive)", () => {
    const tree = makeTree(requests);
    // lowercase query matches capitalized name
    const filtered = filterTree(tree, "login");
    const auth = filtered.children.get("auth");
    expect(auth).toBeDefined();
    expect(auth!.children.size).toBe(1);
    expect(auth!.children.get("login.wire.yaml")).toBeDefined();
    expect(filtered.children.has("users")).toBe(false);
    expect(filtered.children.has("health.wire.yaml")).toBe(false);

    // uppercase query also matches
    const filtered2 = filterTree(tree, "LOGIN");
    expect(filtered2.children.get("auth")!.children.size).toBe(1);
  });

  it("keeps folders that have matching descendants", () => {
    const tree = makeTree(requests);
    const filtered = filterTree(tree, "list");
    expect(filtered.children.has("users")).toBe(true);
    expect(filtered.children.get("users")!.children.size).toBe(1);
    expect(filtered.children.get("users")!.children.has("list.wire.yaml")).toBe(true);
    // Non-matching folders excluded
    expect(filtered.children.has("auth")).toBe(false);
  });

  it("returns empty tree when no matches", () => {
    const tree = makeTree(requests);
    const filtered = filterTree(tree, "nonexistent");
    expect(filtered.children.size).toBe(0);
  });

  it("matches partial and infix text", () => {
    const tree = makeTree(requests);
    // prefix match
    const filtered = filterTree(tree, "heal");
    expect(filtered.children.has("health.wire.yaml")).toBe(true);
    expect(filtered.children.size).toBe(1);

    // infix match ("Check" inside "Health Check")
    const filtered2 = filterTree(tree, "check");
    expect(filtered2.children.has("health.wire.yaml")).toBe(true);
    expect(filtered2.children.size).toBe(1);
  });

  it("returns full tree for whitespace-only query", () => {
    const tree = makeTree(requests);
    const filtered = filterTree(tree, "   ");
    expect(filtered.children.size).toBe(tree.children.size);
  });
});

describe("formatTimeAgo", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("returns 'just now' for timestamps within 60 seconds", () => {
    const now = new Date().toISOString();
    expect(formatTimeAgo(now)).toBe("just now");
  });

  it("returns minutes for timestamps 1-59 minutes ago", () => {
    const fiveMinAgo = new Date(Date.now() - 5 * 60 * 1000).toISOString();
    expect(formatTimeAgo(fiveMinAgo)).toBe("5m ago");
  });

  it("returns hours for timestamps 1-23 hours ago", () => {
    const threeHoursAgo = new Date(
      Date.now() - 3 * 60 * 60 * 1000
    ).toISOString();
    expect(formatTimeAgo(threeHoursAgo)).toBe("3h ago");
  });

  it("returns days for timestamps 24+ hours ago", () => {
    const twoDaysAgo = new Date(
      Date.now() - 2 * 24 * 60 * 60 * 1000
    ).toISOString();
    expect(formatTimeAgo(twoDaysAgo)).toBe("2d ago");
  });
});

describe("METHOD_COLORS", () => {
  it("has colors for all standard HTTP methods", () => {
    expect(METHOD_COLORS.GET).toBeDefined();
    expect(METHOD_COLORS.POST).toBeDefined();
    expect(METHOD_COLORS.PUT).toBeDefined();
    expect(METHOD_COLORS.PATCH).toBeDefined();
    expect(METHOD_COLORS.DELETE).toBeDefined();
  });

  it("returns hex color strings", () => {
    for (const color of Object.values(METHOD_COLORS)) {
      expect(color).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });
});

describe("statusColor", () => {
  it("returns green for 2xx", () => {
    expect(statusColor(200)).toBe("#4ec9b0");
    expect(statusColor(201)).toBe("#4ec9b0");
    expect(statusColor(204)).toBe("#4ec9b0");
  });

  it("returns yellow for 3xx", () => {
    expect(statusColor(301)).toBe("#dcdcaa");
    expect(statusColor(304)).toBe("#dcdcaa");
  });

  it("returns red for 4xx and 5xx", () => {
    expect(statusColor(400)).toBe("#f44747");
    expect(statusColor(404)).toBe("#f44747");
    expect(statusColor(500)).toBe("#f44747");
  });
});

describe("formatBody", () => {
  it("pretty-prints valid JSON", () => {
    const result = formatBody('{"a":1,"b":2}');
    expect(result).toBe('{\n  "a": 1,\n  "b": 2\n}');
  });

  it("returns raw string for non-JSON", () => {
    const html = "<html><body>Hello</body></html>";
    expect(formatBody(html)).toBe(html);
  });

  it("handles empty string", () => {
    expect(formatBody("")).toBe("");
  });
});
