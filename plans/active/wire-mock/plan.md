# wire mock — contract-accurate local mock server (wire-ix7, WEDGE)

## Intent
`wire mock` starts a local HTTP server that answers requests matching the
collection's endpoints, returning the saved **snapshot** for that endpoint when
present, else **schema-shaped** data, else a minimal placeholder. The mock can't
drift from the contract because it IS the collection (`.wire/` files). Lets
frontends/agents build against the API before the backend exists.

## Approach (incremental)
1. **wire-core `mock` module** (this slice — pure + tested):
   - `route_matches(pattern, path)` — `{param}` segments are wildcards.
   - `resolve(collection, wire_dir, method, path) -> Option<MockResponse>` —
     match by method + normalized route; body from snapshot → schema → `{}`.
   - `schema_to_json(&response_schema)` — sample values per type hint.
   - Reuse `drift::normalize_route` (made `pub`) for route patterns.
2. **CLI `wire mock [dir] [--port] [--env]`** (next slice) — a tiny sync HTTP
   server (tiny_http) calling `mock::resolve`; prints the route table on start.
3. **GUI toggle** (later) — "Mock server" start/stop in the app.

## Decisions
- Snapshot first (most accurate); schema gen second; `{}` last. POST→201 else 200.
- Match is method + path; literal segments compared case-insensitively (routes are
  lowercased), `{param}` values ignored. First match wins.
- Mock is approximate by design — it surfaces shape, not business logic.

## Acceptance (slice 1)
- [ ] `mock::resolve` returns snapshot body/status when a snapshot exists.
- [ ] Falls back to schema-shaped JSON, then `{}`.
- [ ] `route_matches` handles params, segment-count, case-insensitivity.
- [ ] `cargo test -p wire-core` green; clippy clean.
