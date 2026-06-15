# wire-web

A web playground for Wire. It wraps `wire-core` in an [axum](https://github.com/tokio-rs/axum)
server and serves the existing React UI, so anyone can try Wire's features in a
browser without installing the desktop app.

This is **Phase 1** of the web showcase: the server scaffold, the transport that
lets the unmodified UI talk to it over HTTP, and a first end-to-end path
(open a sample collection, send requests, generate a collection from a sample
codebase, and detect drift).

## How it works

- **Command adapter.** Every Tauri command in `wire-app` is mirrored as a JSON
  `POST /api/<command>` endpoint. The React UI talks to the backend through a
  single `invoke()` shim (`ui/src/api/invoke.ts`) that uses Tauri IPC in the
  desktop build and `fetch` in the browser — the rest of the app is unchanged.
- **Per-visitor sandbox.** A `wire_session` cookie keys an isolated, ephemeral
  sandbox directory seeded with sample collections and a sample source project.
  All client-supplied paths are confined to that sandbox.
- **Bundled demo API.** Requests are executed server-side against a small
  in-memory API mounted at `/demo` (a pet store plus a few httpbin-style
  endpoints). An egress guard rejects requests to any other host, so the server
  is never an open proxy.
- **Real codebase features.** Scanning, generation, and drift detection run the
  actual `wire-core` engine against the seeded sample project — nothing is
  faked.

## Running locally

The server serves a pre-built UI bundle, so build the UI first:

```bash
# 1. Build the UI (outputs to ui/dist)
cd ui && npm install && npm run build && cd ..

# 2. Run the server from the repo root
cargo run -p wire-web
```

Then open <http://127.0.0.1:8787>.

## Configuration

| Env var             | Default              | Purpose                                            |
| ------------------- | -------------------- | -------------------------------------------------- |
| `WIRE_WEB_ADDR`     | `127.0.0.1:8787`     | Address to bind.                                   |
| `WIRE_WEB_UI_DIR`   | `ui/dist`            | Directory of the built UI to serve.                |
| `WIRE_WEB_DEMO_URL` | `http://127.0.0.1:<port>/demo` | Base URL the demo API is reachable at (and the only allowed request target). |

## Known limitations (to address in later phases)

- The egress guard is enforced on the interactive send paths; request **chains**
  execute seeded files that target `{{base_url}}` but are not yet individually
  egress-checked.
- `$aws:` / `$vault:` secret resolvers shell out to external CLIs and won't
  resolve in the sandbox; `$env:` and `$dotenv:` work.
- Sessions and their sandboxes are in-memory / on-disk and not yet swept on
  expiry.
