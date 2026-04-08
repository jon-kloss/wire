# Plan: VS Code Extension for Wire

**Epic:** wire-vj7
**Status:** Approved
**Date:** 2026-04-08

## Problem

Wire's CLI-first approach is powerful for AI agents but creates friction for developers who prefer visual tools. The criticism "YAML is more work than a GUI" is valid for discovery and quick testing. Developers need a visual layer that sits on top of Wire's file-based collections without replacing them.

## Goals

- Provide a full-featured visual API client inside VS Code that reads/writes `.wire.yaml` files
- Make Wire accessible to developers who prefer GUIs over CLIs
- Maintain perfect file-level compatibility between CLI, desktop app, and extension
- Ship all 13 epic success criteria in v1 (no phased releases)
- Publish to VS Code Marketplace as "Wire"

## Anti-Goals

- Do NOT reimplement Wire's complex operations in TypeScript (chains, drift, breaking, secrets)
- Do NOT create a proprietary storage format — all state lives in `.wire.yaml` files
- Do NOT require the Tauri desktop app
- Do NOT reuse the Tauri app's React UI — build fresh for VS Code's constraints
- Do NOT build a language server for v1 (JSON Schema for `.wire.yaml` autocomplete can come later)
- Do NOT support Open VSX registry in v1 (can add later)

## Constraints

- Extension must work without internet after initial wire CLI download
- Wire CLI binary must be auto-downloaded on first activation (not bundled in VSIX)
- All `.wire.yaml` file changes from CLI or AI agents must be reflected in the UI via file watchers
- Sidebar width is ~300-400px — all UI must work at that width
- VS Code Webview UI Toolkit is deprecated (Jan 2025) — do not use it

## Research Notes

### Sources

1. **VS Code UX Guidelines** (code.visualstudio.com/api/ux-guidelines) — Tree Views for hierarchical data, Webviews for custom forms. Webview UI Toolkit deprecated Jan 2025.
2. **httpYac, Thunder Client, Bruno, REST Client** — All successful VS Code API clients embed HTTP execution directly. CLI wrapping is not used by any market leader. httpYac uses `got`, Thunder Client uses embedded JS + Flexbox.
3. **VS Code Testing API** (code.visualstudio.com/api/extension-guides/testing) — TestController API for native test explorer integration. Available since v1.59. Third-party Test Explorer extension deprecated.
4. **VS Code LSP Guide** (code.visualstudio.com/api/language-extensions/language-server-extension-guide) — LSP is overkill for v1. Red Hat YAML Language Server + JSON Schema covers basic validation.
5. **VS Code Publishing Guide** (code.visualstudio.com/api/working-with-extensions/publishing-extension) — Platform-specific packages supported since v1.61.0. Personal Access Token required.
6. **Node.js child_process** (nodejs.org/api/child_process) — `spawn()` for streaming, `exec()` for buffered. Used by Continue.dev for CLI wrapping.

### Key Insight

Every successful VS Code API client embeds HTTP execution. Wire's uniqueness (chains, drift, breaking, snapshot diffing, AWS/Vault secrets) makes full embedding impractical, but embedding the common path (simple send + test) matches industry best practice while keeping complex operations in Rust.

## Chosen Approach: Pure CLI Wrapper (revised from Hybrid)

**Architecture change (2026-04-08):** Dropped embedded HTTP sender in favor of pure CLI delegation. The wire CLI already handles all execution with JSON output. Embedding HTTP + assertions in TypeScript would duplicate logic and create two paths to maintain forever. The ~50-100ms subprocess overhead per request is acceptable.

```
TypeScript (visual layer only):
  ├── YAML parsing (.wire.yaml files — for tree view, editing, GUI forms)
  ├── Template resolution (for UI: inheritance chain display, header sources)
  ├── Variable extraction (for autocomplete: list available {{vars}})
  └── Response display + JSON tree viewer

ALL execution via wire CLI (-o json):
  ├── wire send <file> -o json          (request execution)
  ├── wire test <path> -o json          (test assertion evaluation)
  ├── wire chain run <file> -o json     (chain execution)
  ├── wire drift <dir> -o json          (drift detection)
  ├── wire breaking -o json             (breaking change detection)
  ├── wire send <file> --snapshot       (snapshot management)
  ├── wire history -o json              (request history)
  ├── wire generate <dir> -o json       (collection generation)
  └── wire env check -d <dir>           (secret validation)
```

### Wire CLI Distribution

- On activation, check: `wire` in PATH, then `~/.wire/bin/wire`
- If not found, download from GitHub releases for user's platform
- Show progress notification during download
- Store in `~/.wire/bin/wire` (or platform equivalent)
- Support auto-update checks (compare local version vs latest release)

### UI Architecture

- **Framework:** React (for webview panels)
- **Build:** esbuild or Vite, single JS bundle per webview
- **Styling:** VS Code CSS variables for native theme integration, Codicons for icons
- **Tree View:** Native VS Code TreeDataProvider for collection browser
- **Webview Panels:** React for request builder, response viewer, chain runner, drift panel
- **State:** Webview ↔ Extension communication via `postMessage` / `onDidReceiveMessage`

## Rejected Alternatives

### 1. Pure CLI Wrapper
Wrap every operation through `wire <cmd> -o json`. Simpler extension code but adds ~50-100ms latency per send. Research shows no successful VS Code API client uses this pattern for HTTP execution. Rejected for UX reasons.

### 2. Full Embedded (No CLI)
Reimplement all Wire logic in TypeScript. Maximum performance but enormous effort. Secret resolution (AWS Secrets Manager, HashiCorp Vault) would need Node.js SDK dependencies. Chain execution, drift detection, and breaking change detection are complex. Would create an ongoing sync burden between Rust and TypeScript implementations. Rejected for maintainability.

### 3. Reuse Tauri App React UI
The Tauri app's React UI (`ui/`) was designed for an 800px+ desktop window. VS Code sidebars are 300-400px. Layout assumptions, state management (Tauri invoke vs postMessage), and component sizing all differ. Porting would require more rework than starting fresh. Rejected for effort/fit.

### 4. Platform-Specific VSIX Bundles
Bundle compiled wire binary per OS/arch (~8MB per platform). Zero setup friction but increases build complexity (6-target build matrix) and package size. Auto-download is more common in the ecosystem and allows binary updates independent of extension updates. Rejected for distribution simplicity.

## Complete Wire Feature → Extension Panel Map

Every Wire CLI feature must have a visual counterpart in the extension.

### Panel 1: Collection Browser (Tree View — sidebar)
| Wire Feature | Extension UI |
|---|---|
| `wire list` | Tree view: folders → requests, grouped by subfolder |
| Collection auto-discovery | Detect all `.wire/` dirs in workspace on activation |
| File watching | `FileSystemWatcher` on `**/*.wire.yaml` — refresh tree on change |
| Create/edit/delete requests | Right-click context menu: New Request, New Folder, Rename, Delete |
| Request color coding | Tree item icons colored by HTTP method (GET=green, POST=yellow, etc.) |

### Panel 2: Request Builder + Response Viewer (Webview — editor area)
| Wire Feature | Extension UI |
|---|---|
| `wire send` | Method dropdown, URL input, Send button |
| Headers | Editable key-value table with add/remove rows |
| Query params | Editable key-value table |
| Body (json/text/formdata) | Tab selector for body type + Monaco editor for JSON, textarea for text, k-v table for formdata |
| `extends: template` | Template selector dropdown, shows inherited headers grayed out |
| Variable interpolation `{{var}}` | Autocomplete for `{{` showing available env variables |
| Response display | Status badge (color-coded), headers table, JSON tree with collapse/expand, raw view toggle |
| Response timing | Elapsed ms, response size displayed in status line |

### Panel 3: Test Results (Webview — below response, or Testing API)
| Wire Feature | Extension UI |
|---|---|
| `wire test` (single file) | Run Tests button on request builder, inline pass/fail per assertion |
| `wire test` (directory) | Run All Tests command, results in VS Code Testing API explorer |
| 15+ assertion operators | Visual assertion builder: field picker, operator dropdown, value input |
| `elapsed_ms less_than` | Performance assertions with visual threshold indicator |
| Test pass/fail | Green checkmark / red X per assertion, summary counts |

### Panel 4: Snapshot Diff (Webview — editor area)
| Wire Feature | Extension UI |
|---|---|
| `wire send --snapshot` | "Save Snapshot" button after sending a request |
| `wire test --snapshot` | "Compare Snapshot" button, side-by-side diff view |
| Snapshot ignore rules | Visual ignore rule editor: click a field path to add to `snapshot.ignore` |
| Structural JSON diff | Color-coded diff: green=added, red=removed, yellow=changed, gray=ignored |
| `wire snapshot update` | "Update Snapshot" button to overwrite golden file |

### Panel 5: Chain Runner (Webview — editor area)
| Wire Feature | Extension UI |
|---|---|
| `wire chain run` | Play button, step-by-step execution with progress |
| Chain step results | Each step shows: request name, status, elapsed time, pass/fail |
| `extract:` variables | Extracted variables displayed per step, highlighted when used in next step |
| `persist: true` | Indicator showing which extractions persist to env file |
| Chain halt on failure | Red stop indicator on failed step, subsequent steps grayed out |
| Chain file editing | Visual chain builder: drag-and-drop step ordering, add/remove steps |

### Panel 6: Environment Manager (Webview — sidebar or editor)
| Wire Feature | Extension UI |
|---|---|
| `active_env` in wire.yaml | Status bar env switcher dropdown |
| Environment variables | Editable key-value table per env file |
| Secret references | Lock icon for `$env:/$dotenv:/$aws:/$vault:` values, type selector dropdown |
| `wire env check` | "Validate Secrets" button — shows which secrets resolve, which fail |
| Create/edit/delete envs | New Environment, Rename, Delete in context menu |
| Variable diff across envs | Side-by-side comparison of variables across environments |

### Panel 7: Template Manager (Webview — editor area)
| Wire Feature | Extension UI |
|---|---|
| Template files | Template list in sidebar under "Templates" folder |
| `extends:` resolution | Visual inheritance chain: template → request, up to 3 levels |
| Header/param merging | Preview of final resolved headers showing which came from template vs request |
| `default_templates` in wire.yaml | Collection-level default template selector |
| Create/edit templates | Template editor: same header/body UI as request builder |
| Usage tracking | "Used by N requests" count with clickable list |

### Panel 8: Drift Detection (Webview — editor area)
| Wire Feature | Extension UI |
|---|---|
| `wire drift` | "Scan for Drift" button, auto-scan on file save (optional) |
| New endpoints (in code, no .wire.yaml) | Green "NEW" badges with one-click "Generate" button |
| Stale endpoints (in .wire, not in code) | Yellow "STALE" badges with one-click "Remove" button |
| Changed endpoints | Blue "CHANGED" badges with diff view (params, body, path) |
| `wire drift --fix` | "Fix All" button to auto-generate/update/remove |
| Framework detection | Shows detected framework (Express, FastAPI, Spring Boot, etc.) |

### Panel 9: Breaking Changes (Webview — editor area)
| Wire Feature | Extension UI |
|---|---|
| `wire breaking --save` | "Save Baseline" button to capture current contract |
| `wire breaking --compare` | "Check Breaking Changes" button |
| Severity classification | Color-coded: red=BREAKING, yellow=WARNING, blue=INFO |
| Change details | Per-endpoint diff: removed fields, type changes, new required params |
| Baseline management | View/update/delete saved baselines |

### Panel 10: Collection Generation (Webview — editor area)
| Wire Feature | Extension UI |
|---|---|
| `wire generate` | "Scan Codebase" button with framework auto-detection |
| Discovered endpoints | Checkbox list of found endpoints, check to include |
| Existing coverage | Indicators showing which endpoints already have .wire.yaml files |
| Preview before write | Preview generated .wire.yaml content before saving |
| Batch generation | "Generate Selected" button for bulk creation |

### Panel 11: History + Replay (Webview — editor area)
| Wire Feature | Extension UI |
|---|---|
| `wire history` | Sortable table: request, method, status, time, size, timestamp |
| History per collection | Filter by collection when multiple .wire/ dirs exist |
| Replay | Click any history entry to re-send that exact request |
| Compare | Select 2+ history entries to diff responses side-by-side |
| `wire history clear` | "Clear History" button with confirmation |

### Command Palette Commands
| Command | Maps to |
|---|---|
| `Wire: Send Request` | Send active .wire.yaml file |
| `Wire: Run Tests` | Test active file or folder |
| `Wire: Run Chain` | Execute active chain file |
| `Wire: Switch Environment` | Quick-pick env selector |
| `Wire: Scan for Drift` | Run drift detection |
| `Wire: Check Breaking Changes` | Run breaking change check |
| `Wire: Generate Collection` | Scan codebase for endpoints |
| `Wire: Save Snapshot` | Save response as golden file |
| `Wire: Compare Snapshot` | Diff current vs saved snapshot |
| `Wire: Show History` | Open history panel |
| `Wire: Validate Secrets` | Check secret references resolve |

## Acceptance Checks

All from the epic (wire-vj7) plus full feature coverage:

- [ ] Extension installs from VS Code Marketplace
- [ ] Sidebar shows collection tree with requests grouped by folder
- [ ] Request builder form sends requests and displays responses
- [ ] Create/edit/save requests updates .wire.yaml files correctly
- [ ] Environment switching works (status bar + panel)
- [ ] Test assertions display pass/fail results (inline + Testing API)
- [ ] Chain execution shows step-by-step results with extractions
- [ ] Drift detection panel shows new/stale/changed with fix actions
- [ ] Breaking change panel shows severity-classified changes
- [ ] Template management (create, edit, assign, see inheritance + usage)
- [ ] Snapshot save/diff/update with ignore rule management
- [ ] Collection generation with preview and batch create
- [ ] History panel with replay and response comparison
- [ ] Secret validation (wire env check) with visual status
- [ ] File changes from CLI/AI agent reflected in sidebar without restart
- [ ] Extension auto-downloads wire CLI if not installed
- [ ] All command palette commands work
- [ ] All tests passing
- [ ] Published to VS Code Marketplace

## Architecture Diagram

```
vscode-wire/
├── package.json              # Extension manifest (contributes, activation)
├── tsconfig.json
├── esbuild.config.js         # Builds extension + webview bundles
├── src/
│   ├── extension.ts          # Activation, command registration
│   ├── cli/
│   │   ├── binary.ts         # Wire CLI discovery, download, version check
│   │   └── runner.ts         # Spawn wire commands, parse JSON output
│   ├── core/
│   │   ├── types.ts          # All Wire types (matching wire-core Rust structs)
│   │   ├── yaml.ts           # .wire.yaml parsing + serialization
│   │   ├── template.ts       # Template resolution (for UI display: inheritance, header sources)
│   │   └── variables.ts      # Variable extraction (for autocomplete: list {{vars}})
│   ├── sidebar/
│   │   ├── CollectionTree.ts # TreeDataProvider for collections
│   │   ├── TreeItems.ts      # TreeItem types (folder, request, template, env, chain)
│   │   └── FileWatcher.ts    # Watch .wire.yaml changes, refresh tree
│   ├── panels/
│   │   ├── RequestPanel.ts   # Webview: request builder + response viewer + test results
│   │   ├── ChainPanel.ts     # Webview: chain runner with step-by-step execution
│   │   ├── SnapshotPanel.ts  # Webview: snapshot diff with ignore rules
│   │   ├── DriftPanel.ts     # Webview: drift detection with fix actions
│   │   ├── BreakingPanel.ts  # Webview: breaking change detection + baseline mgmt
│   │   ├── GeneratePanel.ts  # Webview: collection generation with preview
│   │   ├── EnvPanel.ts       # Webview: environment editor + secret validation
│   │   ├── TemplatePanel.ts  # Webview: template editor + inheritance view
│   │   └── HistoryPanel.ts   # Webview: request history + replay + compare
│   ├── testing/
│   │   └── TestController.ts # VS Code Testing API integration
│   └── env/
│       ├── EnvSwitcher.ts    # Status bar environment selector
│       └── EnvManager.ts     # Read/write envs/*.yaml
├── webview/                  # React app for webview panels
│   ├── package.json          # react, react-dom
│   ├── src/
│   │   ├── index.tsx
│   │   ├── RequestBuilder.tsx    # Method, URL, headers, params, body, template selector
│   │   ├── ResponseViewer.tsx    # Status, headers, JSON tree, raw toggle, timing
│   │   ├── TestResults.tsx       # Inline pass/fail per assertion
│   │   ├── SnapshotDiff.tsx      # Side-by-side diff with ignore rules
│   │   ├── ChainRunner.tsx       # Step-by-step execution, extractions, drag-and-drop editor
│   │   ├── DriftView.tsx         # New/stale/changed with fix actions
│   │   ├── BreakingView.tsx      # Severity-classified changes, baseline management
│   │   ├── GenerateView.tsx      # Endpoint scanner, checkbox picker, preview
│   │   ├── EnvironmentEditor.tsx # Key-value table, secret reference builder, diff across envs
│   │   ├── TemplateEditor.tsx    # Template editor, inheritance chain, usage list
│   │   ├── HistoryView.tsx       # Sortable table, replay, response comparison
│   │   └── components/           # Shared UI components
│   │       ├── HeaderTable.tsx       # Editable key-value table
│   │       ├── ParamTable.tsx        # Query params editor
│   │       ├── BodyEditor.tsx        # JSON/text/formdata tab selector
│   │       ├── JsonTree.tsx          # Collapsible JSON tree viewer
│   │       ├── StatusBadge.tsx       # Color-coded HTTP status badge
│   │       ├── MethodBadge.tsx       # Color-coded HTTP method badge
│   │       ├── AssertionBuilder.tsx  # Field picker + operator dropdown + value input
│   │       ├── DiffViewer.tsx        # Side-by-side structural diff
│   │       └── SecretRefPicker.tsx   # $env/$dotenv/$aws/$vault selector
│   └── styles/
│       └── vscode.css        # VS Code CSS variable bindings
└── test/
    ├── unit/                 # Pure logic tests (yaml, template, variables, assertions)
    ├── integration/          # CLI runner tests (mocked binary)
    └── e2e/                  # VS Code extension host tests
```
