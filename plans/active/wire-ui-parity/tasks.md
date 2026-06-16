# Tasks — Wire GUI parity (all 4 phases)

Decisions: do all 4 phases. Auth + Pre-Run tabs → keep as labeled "coming soon".

## Phase 1 — Backend IPC (wire-app + wire-web, wrap existing core)
- [ ] P1.0 core: add `Serialize` to `SecretCheckResult` (variables/secrets/mod.rs).
- [ ] P1.1 wire-app types: `SnapshotComparison { exists, status_old, status_new, entries: Vec<DiffEntry> }`.
- [ ] P1.2 wire-app commands: `delete_request`, `save_response_snapshot`, `compare_response_snapshot`, `save_breaking_baseline`, `check_breaking`, `check_secrets`.
- [ ] P1.3 wire-app lib.rs: register the 6 new commands.
- [ ] P1.4 wire-web: mirror the 6 commands (sandbox-aware) + routes in main.rs.
- [ ] P1.5 build: `cargo build -p wire-app -p wire-web`; `cargo clippy`; existing tests pass.

## Phase 2 — Surface new backends in UI
- [ ] P2.1 types.ts: SnapshotComparison, BreakingReport/ContractChange, SecretCheckResult.
- [ ] P2.2 Snapshots (A1): real "Save snapshot" + diff on send/tests using save/compare commands (replace the schema-vs-response stopgap).
- [ ] P2.3 Breaking (A2): contract baseline save + check view (severity report).
- [ ] P2.4 Secrets (A3): "Check secrets" button in env UI → resolved/error per var.
- [ ] P2.5 Delete request (A4): delete action on request rows + confirm.

## Phase 3 — UI authoring (IPC already exists)
- [ ] P3.1 Chain builder (A5): add/remove/reorder steps, pick request, define extracts, persist toggle → save via save_request.
- [ ] P3.2 Create env + add variable (A6).
- [ ] P3.3 Body-type selector + form-data editor (A7).
- [ ] P3.4 Snapshot ignore-rules editor (A1 cont.); response_schema editor (A8); clear-history button (C).

## Phase 4 — Stub labels
- [ ] P4.1 Auth + Pre-Run tabs → clear "coming soon" states.

## Status: ALL PHASES IMPLEMENTED + mechanically verified
Phase 1 ✓ wire-app + wire-web IPC (save/compare snapshot, delete_request, save/check breaking, check_secrets).
Phase 2 ✓ UI: real snapshot save+diff; breaking view (baseline save+check) in Drift tab; check-secrets; delete-request.
Phase 3 ✓ chain builder; create-env + add-var; body-type selector + form-data; clear-history.
Phase 4 ✓ Auth + Pre-Run → "Coming soon" states.

## Verify
- [x] cargo build (app+web) ✓; clippy ✓; core 419 tests ✓; wire-web 10 route tests ✓.
- [x] npm run build ✓; lint ✓; vitest 58 ✓; tsc clean.
- [ ] Manual click-through of new flows — needs a running GUI/browser (not available here).
