# Context: VS Code Extension for Wire

## Key Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Architecture | Pure CLI wrapper (revised from hybrid) | Avoids duplicating HTTP + assertion logic in TS; ~50-100ms overhead acceptable; single source of truth for execution |
| Scope | Full epic in v1 | User preference — no phased releases |
| UI framework | React | Familiar from Tauri app, large ecosystem |
| UI reuse | Start fresh | Tauri app designed for 800px+ window, VS Code sidebar is 300-400px |
| Binary distribution | Auto-download on activation | Smaller VSIX, binary updates independent of extension |
| Webview UI Toolkit | Do not use | Deprecated Jan 2025 |
| Test explorer | VS Code Testing API (TestController) | Native, replaces deprecated Test Explorer extension |

## Key Files in Wire Codebase

| File | Relevance |
|------|-----------|
| `crates/wire-core/src/lib.rs` | Core types: WireRequest, WireResponse, TestAssertion, ChainStep |
| `crates/wire-core/src/template.rs` | Template resolution logic (must replicate in TS) |
| `crates/wire-core/src/chain/mod.rs` | Chain execution (delegated to CLI) |
| `crates/wire-cli/src/main.rs` | CLI commands, JSON output format (the extension's API surface) |
| `examples/httpbin/.wire/` | Reference collection for testing |

## CLI JSON Output Contracts

The extension depends on stable JSON output from:
- `wire send <file> -o json` — request/response data
- `wire test <path> -o json` — TestRunSummary with assertions
- `wire chain run <file> -o json` — step-by-step results with extractions
- `wire drift <dir> -o json` — DriftReport with new/stale/changed
- `wire breaking -o json` — BreakingReport with severity classifications
- `wire list <dir> -o json` — collection listing (if supported)
- `wire history -o json` — history entries

**Risk:** JSON output contracts may need hardening before extension relies on them. Verify structure and stability before building parsers.

## TypeScript Surface (visual layer only)

With pure CLI wrapper architecture, TypeScript only handles:

1. **YAML parsing** — read/write `.wire.yaml` files for tree view, editing, GUI forms
2. **Template resolution** — for UI display: inheritance chain visualization, header source tracking
3. **Variable extraction** — for autocomplete: list available `{{var}}` names from env files

All execution (send, test, chain, drift, breaking, etc.) delegates to `wire <cmd> -o json` via the CLI runner. No HTTP sender, assertion evaluation, or secret detection in TypeScript.

## Extension Panel Inventory

11 panels total covering every Wire feature:

| # | Panel | Type | Wire Commands |
|---|-------|------|---------------|
| 1 | Collection Browser | TreeView (sidebar) | `wire list` |
| 2 | Request Builder + Response | Webview (editor) | `wire send` |
| 3 | Test Results | Webview + Testing API | `wire test` |
| 4 | Snapshot Diff | Webview (editor) | `wire send --snapshot`, `wire test --snapshot` |
| 5 | Chain Runner | Webview (editor) | `wire chain run` |
| 6 | Environment Manager | Webview (sidebar/editor) | `wire env check` |
| 7 | Template Manager | Webview (editor) | _(YAML read/write)_ |
| 8 | Drift Detection | Webview (editor) | `wire drift` |
| 9 | Breaking Changes | Webview (editor) | `wire breaking` |
| 10 | Collection Generation | Webview (editor) | `wire generate` |
| 11 | History + Replay | Webview (editor) | `wire history` |

## Implementation Notes

- Extension lives in `vscode-wire/` within the Wire monorepo (same repo)
- Build output: extension.js (462KB), webview.js (194KB) via esbuild
- VS Code engine minimum: ^1.85.0
- Node target: node20 (matching user's v20.20.0)
- `js-yaml` used for YAML parsing (with @types/js-yaml), wrapped in typed `src/core/yaml.ts`
- `undici` used for HTTP (CLI binary download only — no embedded sender)
- All modules now use typed parsers from `src/core/yaml.ts` — no raw js-yaml calls elsewhere
- **Architecture revised to pure CLI wrapper (2026-04-08):** dropped sender.ts, assertions.ts, secrets.ts. All execution via wire CLI -o json.
- `src/core/types.ts` has 30+ types matching wire-core Rust structs exactly (field names, optionality, enums)
- `src/cli/runner.ts` has typed methods for every wire CLI command with JSON output
- FileWatcher debounces at 300ms to batch rapid changes (e.g., from wire generate)
- EnvSwitcher reads/writes active_env via parseCollectionFile/serializeCollection
- Template merging rules from Rust: headers additive (request wins), params additive, body top-level merge, tests concatenated, chain override wins

## Open Questions

- Does `wire list -o json` exist? If not, need to add it or parse YAML directly in the extension.
- Does `wire history -o json` exist? Same question.
- Does `wire generate -o json` exist? Need structured output for the generation preview panel.
- What's the exact JSON schema for `wire send -o json` output?
- Publisher identity — does a VS Code Marketplace publisher exist for Wire?
- Chain builder drag-and-drop — how complex should the visual editor be vs just editing YAML?
- GitHub repo for releases — what URL pattern for binary downloads?
