# Wire — Dark UI Redesign + Accent Theming + Logo

## Intent
Port the design handoff (`design-bundle/`) into the existing `ui/` React+TS+Vite
frontend (shared by the Tauri desktop shell `crates/wire-app/` and the `wire-web`
playground). Refined, dense, dark developer-tool look (Linear/Zed energy) with a
single themeable accent (Ember/Signal/Pulse), a real logo, a new title bar, and
redesigned onboarding / contract-tests+snapshot / drift / chaining views.

Decisions (from user, 2026-06-15):
- **Scope:** everything in one pass (foundation + net-new screens 5–8).
- **Fonts:** self-hosted woff2 in `ui/` (offline-safe for desktop).
- **Icons:** regenerate Tauri bundle icons from `logo.svg` now (`cargo tauri icon`).

## Constraints / ground rules
- No new framework / CSS-in-JS / component kit. Plain CSS in `App.css`, Monaco for editors.
- Recreate the prototype high-fidelity; its mock data is illustrative — keep the app's real data flow.
- `color-mix()` is fine (WebView2/WKWebView are Chromium 111+).
- Every color references `var(--accent*)` / tokens so accent switch needs no React re-render.

## Acceptance contract
- [ ] `tokens.css` imported before `App.css`; fonts self-hosted and applied.
- [ ] Accent picker in title bar live-switches Ember/Signal/Pulse via `html[data-accent]`, persisted to `localStorage["wire.accent"]`, no flash/re-render.
- [ ] Logo mark + wordmark (accent "i") in title bar; favicon + Tauri icons regenerated from `logo.svg`.
- [ ] 44px title bar: lockup, breadcrumb, accent picker, env chip, search + lock icon buttons. Grid below = `236px 1fr 1fr`.
- [ ] All blue literals (`#0e639c/#007acc/#1177bb/#094771`) and surface grays replaced by tokens across App.css + SourceEditor/SampleGallery/PromptModal.
- [ ] `METHOD_COLORS` + `statusColor()` updated to handoff hues (+ info tier).
- [ ] Monaco "wire-dark" theme from syntax tokens; body editor + read-only response use it.
- [ ] Onboarding (no collection) = full 2-column screen per §5, wired to real handlers.
- [ ] Tests tab = 2-col assertion editor + snapshot diff per §6 (snapshot wired to real data — see Risk).
- [ ] Drift tab = summary strip + change list + detail panel per §7, wired to check_drift/fix_drift.
- [ ] Chain view = vertical "wire" with nodes + EXTRACTS/USES chips per §8, wired to runChain/ChainStepResult.
- [ ] `npm run build` (tsc+vite) green; `npm test` green; lint clean.

## Anti-goals
- Don't change backend request/test/chain semantics.
- Don't redesign features that don't exist (no new test runner, etc.) — restyle/visualize existing flows.
- Don't break the wire-web playground build (shared `ui/`).

## Risks
- **Snapshot diff (screen 6) has no UI-exposed backend command.** wire-core supports snapshots; wire-app/wire-web don't expose them. Resolve at screen 6: either add minimal snapshot IPC to both crates, or present the diff panel from available data. Surface to user before adding Rust.
- Breaking-changes data also not exposed; screen 7 maps to existing DriftItem.changes[] (no structured before/after schema) — render from `changes[]`.
- Self-hosted font fetch needs network now (to download woff2 once); vendor into repo.
