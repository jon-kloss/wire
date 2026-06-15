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
  endpoints). Each session's `{{base_url}}` is scoped to `/demo/s/<session-id>`,
  so its demo data is isolated from other visitors. An egress guard rejects
  requests to any other host, so the server is never an open proxy.
- **Real codebase features.** Scanning, generation, and drift detection run the
  actual `wire-core` engine against seeded sample projects for every supported
  framework (Express, FastAPI, ASP.NET Core, Spring Boot, Next.js) — nothing is
  faked.
- **Interactive drift demo.** From the Drift panel, "Edit Source" opens an
  in-browser editor for the scanned project's files. Change a route handler,
  save, and re-check drift to watch Wire detect the difference against the
  generated collection. Backed by the sandbox-confined `list_source_files` /
  `read_source_file` / `save_source_file` endpoints.

## Running locally

The server serves a pre-built UI bundle, so build the UI first:

```bash
# 1. Build the UI (outputs to ui/dist)
cd ui && npm install && npm run build && cd ..

# 2. Run the server from the repo root
cargo run -p wire-web
```

Then open <http://127.0.0.1:8787>.

## Running with Docker

A multi-stage `Dockerfile` builds the UI and the server into one image (the
embedded seeds ship inside the binary). Build from the repo root:

```bash
docker build -f crates/wire-web/Dockerfile -t wire-web .
docker run --rm -p 8787:8787 wire-web
```

The container binds `0.0.0.0:8787` and serves the bundled UI. Behind HTTPS,
also set `WIRE_WEB_SECURE_COOKIE=1`.

## Deploying behind HTTPS

`deploy/docker-compose.yml` runs the playground behind [Caddy](https://caddyserver.com),
which terminates TLS and provisions/renews a Let's Encrypt certificate
automatically. `wire-web` runs HTTP-only on the internal network with
`WIRE_WEB_SECURE_COOKIE=1` set (it's served to browsers over HTTPS).

Point your domain's DNS at the host, then from the repo root:

```bash
WIRE_WEB_DOMAIN=play.example.com \
  docker compose -f crates/wire-web/deploy/docker-compose.yml up -d
```

Caddy exposes ports 80/443 and proxies to the `wire-web` service; the
certificate is obtained on first request and persisted in a named volume. The
proxy also sends an HSTS header. To get Let's Encrypt renewal notices, set
`WIRE_WEB_ACME_EMAIL` and enable the email block in `deploy/Caddyfile`.

## Configuration

| Env var             | Default              | Purpose                                            |
| ------------------- | -------------------- | -------------------------------------------------- |
| `WIRE_WEB_ADDR`     | `127.0.0.1:8787`     | Address to bind.                                   |
| `WIRE_WEB_UI_DIR`   | `ui/dist`            | Directory of the built UI to serve.                |
| `WIRE_WEB_DEMO_URL` | `http://127.0.0.1:<port>/demo` | Base URL the demo API is reachable at (and the only allowed request target). |
| `WIRE_WEB_SESSION_TTL_SECS` | `3600` | Idle lifetime before a session and its sandbox are swept. |
| `WIRE_WEB_SECURE_COOKIE` | `false` | Set the `Secure` attribute on the session cookie (enable behind HTTPS). |

## Known limitations (to address in later phases)

- `$aws:` / `$vault:` secret resolvers shell out to external CLIs and won't
  resolve in the sandbox; `$env:` and `$dotenv:` work.

## Security notes

- **Egress** is enforced inside the shared `HttpClient` (`HttpClient::restricted_to`),
  so every execution path — single sends *and* chains — is confined to the
  bundled demo API and none can bypass it.
- **Sandboxes** are confined per session and the session id is validated before
  it's used as a path component. Idle sessions (and their sandboxes) are swept
  after `WIRE_WEB_SESSION_TTL_SECS` (default 1h).
- Set `WIRE_WEB_SECURE_COOKIE=1` when serving over HTTPS so the session cookie
  carries the `Secure` attribute.
