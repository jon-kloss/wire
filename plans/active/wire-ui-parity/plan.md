# Wire — GUI ↔ feature parity

## Goal
Make the desktop GUI (`ui/`, shared with wire-web) able to do everything the
Wire CLI / wire-core supports. Close the gaps where a real wire feature has no
GUI path. Audit done 2026-06-15 via 4 parallel surface maps (CLI/docs, wire-core,
wire-app IPC, ui surface), then verified against source.

## Surfaces (verified)
- **Registered IPC (wire-app `lib.rs` invoke_handler): 22 commands** — open/create/rename collection,
  read/save request, read/save/delete/toggle template, get/save environment, list environments,
  list/clear history, scan_codebase, check_drift, fix_drift, run_chain, evaluate_tests,
  send_request, send_raw_request. wire-web mirrors these + 4 playground-only (samples, source files).
- **NOT in IPC (but implemented in wire-core):** snapshots, breaking changes, env/secret check, delete-request.

## GAPS

### A. True parity gaps (wire feature exists; GUI can't do it)

**A1 — Snapshots** *(core: `snapshot::save_snapshot/load_snapshot`, `diff::structural_diff`, `SnapshotConfig.ignore`)*
- CLI: `wire send --snapshot`, `wire test --snapshot`, `wire snapshot update`.
- IPC: none. UI: the redesigned tests-tab "snapshot diff" is actually `response_schema` vs response — NOT real golden-file snapshots.
- Need: IPC to save a response snapshot + compare on send/test; "Save snapshot" + real diff (added/removed/changed, strikethrough) in response/tests; per-request `snapshot.ignore` path editor.

**A2 — Breaking changes** *(core: `breaking::save_snapshot` (baseline), `breaking::compare` → BREAKING/WARNING/INFO)*
- CLI: `wire breaking`, `wire breaking --save`.
- IPC: none. UI: the Drift view shows BREAKING/WARNING/INFO but those are mapped from drift categories (new/stale/changed) — NOT the real contract baseline diff.
- Need: IPC (save baseline to `.wire/contract-snapshot.json`, compare) + a contract/breaking view (save baseline button, severity report, before/after).

**A3 — Env / secret check** *(core: `check_collection_secrets`)*
- CLI: `wire env check` — validates `$env/$dotenv/$aws/$vault` resolve (skips live AWS/Vault).
- IPC: none. UI: shows masked values, can't validate.
- Need: IPC + "Check secrets" button (per env) showing resolved/error per var.

**A4 — Delete request file**
- GUI can delete templates + collections but NOT an individual request file (no IPC, no tree affordance).
- Need: `delete_request` IPC + delete action on request rows.

**A5 — Create/edit chains** *(authoring is YAML-only today)*
- CLI/core: chains are first-class; GUI can RUN (`run_chain`) but can't author. `save_request` already persists `chain[]`.
- Need: chain builder in the Chain tab — add step (pick request), define `extract` (name ← body./headers./status), `persist` toggle, reorder/remove.

**A6 — Create environment + add variable**
- UI selects existing envs and edits existing vars only; can't create a new env file or add a new key. `save_environment` IPC already exists; creating a new env = save_environment with a new name.
- Need: "New environment" + "+ add variable" affordances in the env UI.

**A7 — Form-data / multipart body** *(core `BodyType::FormData`; `send_raw_request` handles it)*
- UI only does JSON/text and has no body-type selector; can't build form-data.
- Need: body-type chip (JSON / text / form-data) + key/value editor for form-data.

**A8 — Edit `response_schema`** *(used by breaking + schema-assert)*
- Auto-populated by `generate`; not editable in GUI. `save_request` persists it. Minor.

**A9 — Snapshot-based test run / bulk test** — `wire test <dir>`, `--snapshot`, `-o json`. GUI runs tests inline on a single send; no folder run. Minor.

### B. Stubs for features that DON'T exist in core (product decisions, not parity)
- **Auth tab** — wire-core has NO auth schemes; auth = headers/env/templates. Tab is an empty `<p>Authentication</p>`.
  → Build an auth *helper* (Bearer / Basic / API-key → writes the right header) as a convenience, OR remove the tab. (Not a parity gap.)
- **Pre Run / pre-request scripts** — no scripting engine anywhere in wire. Empty stub.
  → Remove/relabel the tab (honest), or treat as a future core feature (large, separate effort).

### C. Minor / nice-to-have
- Clear-history button in Activity (IPC `clear_history` exists, confirm UI exposes it).
- Global templates (`~/.wire/templates`) browsing; history search/export/replay; `install-claude-skill` button (onboarding shows the command but doesn't run it).

## PHASED PLAN

**Phase 1 — Backend IPC parity** (Rust, wire-app + wire-web, wrapping existing core; low risk):
`save_snapshot`/`test_with_snapshot`, `save_breaking_baseline`/`check_breaking`, `check_secrets`, `delete_request`. Mirror in wire-web. Add types to ui `types.ts`.

**Phase 2 — Surface new backends in UI:** real Save-snapshot + snapshot diff on send/test (A1); breaking/contract view (A2); Check-secrets button (A3); delete-request in tree (A4).

**Phase 3 — UI-only authoring gaps** (IPC already exists): chain builder (A5); create-env + add-var (A6); body-type selector + form-data editor (A7); snapshot ignore editor (A1 cont.); response_schema editor (A8); clear-history (C).

**Phase 4 — Stub decisions:** Auth helper or remove (B); remove/relabel Pre-run (B).

## Acceptance
- Every CLI/core feature reachable from the GUI, or explicitly decided out-of-scope.
- `npm run build`/test/lint green; wire-app + wire-web build; no regressions.

## Open questions for user
- Priority/order of phases? Do all, or Phase 1+2 first (the real parity blockers)?
- Auth tab: build header-helper vs remove? Pre-run tab: remove vs "coming soon"?
