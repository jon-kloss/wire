# Context / Discovery Log

## Codebase map (verified)
- `ui/` shared frontend. Entry `main.tsx` → `App.tsx` (2340 lines) + `App.css` (2067 lines).
- `ui/src/main.tsx`: imports `./index.css` then `App` (App imports `./App.css`). Add `tokens.css` import before App.
- `ui/index.html`: `<title>ui</title>`, favicon `/favicon.svg`. No font links yet.
- `ui/src/utils.ts`: `METHOD_COLORS`, `statusColor()`, `buildTree/filterTree/formatTimeAgo/formatBody`.
- `ui/src/types.ts`: IpcResponse, WireRequest, Assertion, TestResult, DriftItem/DriftReport, ChainStepResult/ChainResult, IpcCollectionInfo, etc.
- `ui/src/api/invoke.ts`: `invoke()` (Tauri IPC or HTTP to wire-web), `open()`, `isWebPlayground()`.
- Components: `SampleGallery.tsx`, `SourceEditor.tsx`, `PromptModal.tsx` (web-playground modals).
- Monaco: `@monaco-editor/react` lazy-loaded; body editor at App.tsx ~1905, theme `vs-dark`.
- Tauri shell: `crates/wire-app/tauri.conf.json` (frontendDist ../../ui/dist, title "Wire", icons list), `crates/wire-app/icons/`.
- `cargo tauri` v2.10.1 available → `cargo tauri icon <png>` regenerates set. No rsvg/imagemagick/inkscape; rasterize via temp `sharp` in ui/.

## App.tsx structure (current)
- Root `<div class="app">` = CSS grid `240px 1fr 1fr` containing: SampleGallery, optional SourceEditor, `<aside.sidebar>`, `<main.request-builder>`, `<section.response-viewer>`, PromptModal.
- Sidebar: new-request-btn, sidebar-tabs (collections/activity/drift), collection accordions (env + templates + request tree via `TreeItem`), activity (history), drift panel.
- Request builder: url-bar (method-select, url-input w/ `{{var}}` highlighter, send/chain/save/secrets/save-as-template), template-picker, request-tabs (query/headers/auth/body[Monaco]/tests/pre-run).
- Response viewer: panel-header (status badge, ms, bytes), tabs (body[pre]/headers), chain-preview + chain-results.
- State: accent NOT present yet → add `useAccent()`. All else stays.

## Backend IPC commands exposed (wire-app invoke_handler)
check_drift, clear_history, create_collection_cmd, delete_template, evaluate_tests,
fix_drift, get_environment, list_environments, list_history, list_templates_cmd,
open_collection, read_request, read_template, run_chain, save_environment,
save_request, save_template, scan_codebase, send_raw_request, send_request,
toggle_default_template.
→ NO snapshot / breaking commands exposed to UI (CLI has them). See plan Risk.

## Token highlights (design-bundle/tokens.css)
- Accent vars only: --accent/-bright/-ink/-grad; tints via color-mix at use-site.
- Default Ember #ff6a4d; Signal #b8ec4f; Pulse #1fc6e8.
- Method: GET #5ec98c POST #e7b35a PUT #5b9bf0 PATCH #c98bdb DELETE #ec6a5e.
- Status: ok #5ec98c warn #e7b35a err #ec6a5e info #7fafe0.
- Syntax: key #7fafe0 string #9fcf86 number #d8a657 punct #5b6068.

## Decisions / notes
- Logo+accent components → new `ui/src/brand.tsx` (adapt from design-bundle/accent-and-logo.example.tsx).
- Response body: upgrade `<pre>` → Monaco read-only with wire-dark theme (handoff §4).
- Onboarding: render full-screen 2-col when `collections.length === 0` (replaces in-sidebar empty-state as the primary empty view).
