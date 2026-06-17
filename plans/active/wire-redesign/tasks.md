# Tasks — Wire Redesign

## Now (Phase 0 — Foundation)
- [ ] F1. Self-host fonts: download Schibsted Grotesk + JetBrains Mono woff2 into `ui/public/fonts/`, add `@font-face` (in tokens.css or fonts.css).
- [ ] F2. Add `ui/src/tokens.css` (from bundle), import in `main.tsx` before App.
- [ ] F3. `ui/src/brand.tsx`: `useAccent()`, `WireMark/WireWordmark/WireLockup`, `AccentPicker`. Add `logo.svg` to assets.
- [ ] F4. `utils.ts`: update `METHOD_COLORS` + `statusColor()` → token hues (+ info), prefer reading from CSS vars where practical.

## Next (Phase 1 — Shell + title bar)
- [ ] S1. App.tsx root → flex column `[titlebar][grid flex:1]`; `.app` grid `236px 1fr 1fr`.
- [ ] S2. Title bar: lockup, breadcrumb (selected request path), AccentPicker, env chip, search + lock buttons.

## Then (Phase 2 — Re-token existing screens)
- [ ] R1. App.css: replace blue literals + surface grays w/ tokens; new titlebar/selected-row/tab/pill rules; sidebar, request builder, response, tests, drift, chain, prompt, sample gallery, source editor.
- [ ] R2. Monaco `wire-dark` theme; apply to body editor + read-only response body.

## Then (Phase 3 — Net-new views)
- [ ] N1. Onboarding 2-col screen (no collection open) per §5.
- [ ] N2. Tests tab: 2-col assertion editor + snapshot diff per §6 (resolve snapshot backend first).
- [ ] N3. Drift tab → summary strip + change list + detail panel per §7.
- [ ] N4. Chain "wire" visualization per §8.

## Then (Phase 4 — Brand assets)
- [ ] B1. favicon.svg from logo; `index.html` title "Wire".
- [ ] B2. Regenerate Tauri icons from logo.svg (`cargo tauri icon`).

## Verify
- [x] V1. `npm run build` (tsc+vite) ✓, `npm test` 58/58 ✓, `npm run lint` ✓. Bundle confirmed: all 3 accent themes, new classes, self-hosted fonts, regenerated icons.
- [ ] V2. Manual visual pass (needs a browser): `cd ui && npm run dev`, switch accents, walk each screen. Not run here (no browser tooling).

## Done
- F1–F4 foundation: self-hosted fonts; tokens.css; accent.ts + brand.tsx + logo.svg; method/status token colors (+ formatBytes).
- S1/S2 app-shell + 236/1fr/1fr grid; 44px title bar (lockup, breadcrumb, accent picker, env chip+menu, search/lock).
- R1 full App.css token rewrite; R2 monaco "wire-dark" theme on body + read-only response.
- N1 onboarding 2-col; N2 tests 2-col (assertions | schema-vs-response diff); N3 drift main-panel view; N4 chain "wire" viz.
- B1 favicon = Wire mark; title "Wire". B2 Tauri icons regenerated (sharp --no-save; manifests untouched).

## Known gap / follow-up
- Snapshot persistence (§6 "Update snapshot") has NO UI-exposed backend. Right tests column shows a real schema-vs-response contract diff (or response well) instead. True snapshot save/compare needs new IPC in wire-app + wire-web over wire-core. USES chips on chain steps omitted (no raw {{var}} at result time); EXTRACTS shown (real).
