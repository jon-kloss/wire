# Tasks: VS Code Extension for Wire

## Now

- [ ] Test results panel — VS Code Testing API integration (TestController) for `wire test` on directories + Run All Tests
- [ ] Chain runner panel — step-by-step execution, per-step results, extracted variables display

## Next

### Snapshots (Panel 4)
- [ ] Save Snapshot button after sending
- [ ] Compare Snapshot — side-by-side structural JSON diff
- [ ] Ignore rule editor — click field path to add to `snapshot.ignore`
- [ ] Color-coded diff — added/removed/changed/ignored
- [ ] Update Snapshot button

### Environment Manager (Panel 6)
- [ ] Editable key-value table per env file
- [ ] Secret reference picker — `$env:/$dotenv:/$aws:/$vault:` selector with lock icon
- [ ] Validate Secrets button — calls `wire env check`, shows resolve status
- [ ] Create/edit/delete environments
- [ ] Side-by-side variable comparison across environments

### Template Manager (Panel 7)
- [ ] Template list in sidebar under "Templates"
- [ ] Template editor — same header/body UI as request builder
- [ ] Inheritance chain visualization — template → request, up to 3 levels
- [ ] Resolved headers preview — which came from template vs request
- [ ] Collection-level `default_templates` selector in wire.yaml
- [ ] Usage tracking — "Used by N requests" with clickable list

### Drift Detection (Panel 8)
- [ ] "Scan for Drift" button — calls `wire drift -o json`
- [ ] New/stale/changed badges with one-click fix actions
- [ ] Framework detection display
- [ ] "Fix All" button — calls `wire drift --fix`
- [ ] Optional auto-scan on file save

### Breaking Changes (Panel 9)
- [ ] "Save Baseline" button — calls `wire breaking --save`
- [ ] "Check Breaking Changes" button — calls `wire breaking -o json`
- [ ] Severity-classified display — BREAKING (red), WARNING (yellow), INFO (blue)
- [ ] Per-endpoint change details
- [ ] Baseline management — view/update/delete

### Collection Generation (Panel 10)
- [ ] "Scan Codebase" button — calls `wire generate -o json`
- [ ] Checkbox list of discovered endpoints
- [ ] Existing coverage indicators
- [ ] Preview generated `.wire.yaml` before writing
- [ ] Batch generation button

### History + Replay (Panel 11)
- [ ] Sortable history table — request, method, status, time, size, timestamp
- [ ] Filter by collection
- [ ] Click to replay any history entry
- [ ] Select 2+ entries to diff responses side-by-side
- [ ] Clear History button

### Command Palette & Polish
- [ ] All 11 command palette commands — implement remaining stubs
- [ ] Create/edit/delete requests from GUI → write .wire.yaml files (partially done)
- [ ] Keyboard shortcuts for common actions (Send, Switch Env)

## Later

- [ ] Publish to VS Code Marketplace
- [ ] Extension tests — unit (core engine), integration (CLI runner), e2e (extension host)
- [ ] Auto-update check for wire CLI binary
- [ ] JSON Schema for `.wire.yaml` autocomplete (via `yaml.schemas` setting)
- [ ] CodeLens on `.wire.yaml` files — inline "Send" / "Test" buttons
- [ ] Gutter decorations for test pass/fail in `.wire.yaml` files

## Blocked

_(none)_

## Done

- [x] Scaffold extension project — package.json, tsconfig, esbuild, directory structure
- [x] CLI binary discovery and auto-download (`src/cli/binary.ts`)
- [x] Core types (`src/core/types.ts`) — 30+ TypeScript types matching wire-core Rust structs
- [x] YAML parser (`src/core/yaml.ts`) — typed parsing/serialization for WireRequest, WireCollection, Environment
- [x] CLI runner (`src/cli/runner.ts`) — typed methods for all wire commands
- [x] Template resolution (`src/core/template.ts`) — recursive extends, merge rules, circular detection
- [x] Variable interpolation (`src/core/variables.ts`) — VariableScope, interpolate, extractVariableNames
- [x] Collection tree sidebar (`src/sidebar/CollectionTree.ts`) — tree view, method icons, context menus (new/delete/rename)
- [x] File watcher (`src/sidebar/FileWatcher.ts`)
- [x] Environment switcher (`src/env/EnvSwitcher.ts`)
- [x] Environment manager (`src/env/EnvManager.ts`)
- [x] **Request builder webview** (`src/panels/RequestPanel.ts` + `webview/src/RequestBuilder.tsx`) — method/URL/headers/params/body/tests tabs, Send + Test buttons, extends badge
- [x] **Response viewer** (`webview/src/ResponseViewer.tsx`) — status badge, JSON tree (collapsible), raw view, headers view, timing/size
- [x] **Test results** (`webview/src/TestResults.tsx`) — inline pass/fail per assertion, summary counts
- [x] Context menus — new request (with prompts), new folder, delete (with modal), rename
- [x] Request panel opens from tree clicks + command palette
- [x] Build passes, TypeScript type-checks clean (ext 473KB, webview 207KB)

## Dropped

- ~~Embedded HTTP sender (`src/core/sender.ts`)~~ — pure CLI wrapper architecture
- ~~Test assertion evaluation (`src/core/assertions.ts`)~~ — delegates to `wire test -o json`
- ~~Secret detection (`src/core/secrets.ts`)~~ — not needed with pure CLI wrapper
