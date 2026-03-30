# Wire

A fast, local-first API client built with Tauri, Rust, and React.

Wire stores requests as human-readable YAML files that live in your repo alongside your code. No accounts, no cloud sync, no bloat.

## Built for AI-Assisted Development

Wire is designed to work with [Claude Code](https://claude.ai/claude-code) and AI coding agents. While other API clients lock your data in GUIs and proprietary formats, Wire stores everything as YAML files and exposes every feature through a CLI — making it fully accessible to AI agents using standard file and shell tools.

```bash
wire install-claude-skill
```

One command installs a skill that teaches Claude Code how to use Wire. From that point on, Claude will automatically:

- **Generate collections from your codebase** — scan your routes and create `.wire.yaml` files for every endpoint
- **Write contract tests** — add declarative test assertions to verify response shapes, status codes, and timing
- **Run tests in CI** — `wire test .wire/ -e ci` with exit codes for pass/fail
- **Chain requests** — build multi-step auth flows where tokens pass between requests
- **Detect endpoint drift** — compare your code against your collection to find missing or stale tests
- **Manage environments and secrets** — set up dev/staging/prod configs with secret injection

The skill is embedded in the Wire binary — no extra files or downloads. Works out of the box.

## Why Wire?

- **Local-first** — your data stays on your machine, no login required
- **Human-readable files** — `.wire.yaml` files are diffable in git and editable in any text editor
- **Fast** — Tauri + Rust backend, not Electron
- **CLI from day one** — same core library powers both the GUI and the `wire` CLI
- **AI-native** — YAML files + CLI = fully accessible to AI coding agents without browser automation or proprietary APIs

### What Wire catches that other tests don't

Unit tests verify your code logic. Integration tests verify your components work together. Both run **from the inside** of your application — they import your code, call your functions, and assert on return values.

Wire tests hit your API **from the outside**, the same way a real client would. They catch the class of bugs that live in the seam between "code works correctly" and "HTTP response matches what consumers expect":

- All tests green, but `POST /api/users` returns `user_name` instead of `userName` because someone renamed a serialization attribute
- The error response changed from `{"error": "msg"}` to `{"message": "msg"}` and the frontend can't parse it
- A middleware change quietly dropped a required response header
- The endpoint still returns 200 but pagination cursors are in a different format

These are **contract bugs** — your code does the right thing internally, but the HTTP interface changed in a way that breaks consumers. Unit tests can't see HTTP responses. Integration tests *can* but rarely assert on exact response shapes.

### Wire as part of your test strategy

| Question | Tool |
|----------|------|
| Does my validation logic reject invalid emails? | Unit test |
| Does my service layer create a user and send a welcome email? | Integration test |
| Does `POST /api/users` return `{"id": int, "email": string}` with a 201? | **Wire test** |
| Did someone change the response shape without realizing it? | **Wire snapshot** |
| Does the same endpoint behave identically in dev and staging? | **Wire diff** |
| Did a new route get added without corresponding API tests? | **Wire drift** |

Wire isn't a replacement for unit or integration tests — it fills the gap between "code works" and "API behaves as expected."

## Features

- Send HTTP requests (GET, POST, PUT, PATCH, DELETE) from a three-panel GUI
- Create, rename, and remove collections from the GUI
- Save requests as `.wire.yaml` files — into a collection or standalone
- Multiple collections displayed as expandable accordions with color-coded HTTP method badges
- Collections dropdown menu for creating, importing, and managing collections
- **Generate from Codebase** — auto-discover HTTP endpoints from ASP.NET (controllers + minimal APIs) and Express/Node projects
- Environment variables with `{{variable}}` interpolation and scoping (Global > Environment > Collection > Request)
- Environment switching (dev/staging/prod)
- **Secret injection** — reference secrets from env vars, `.env` files, AWS Secrets Manager, or HashiCorp Vault without storing plaintext
- **Request chaining** — define multi-step API flows where responses feed into subsequent requests
- **Request templates** — define shared headers, auth, and base URLs in `.wire/templates/` and inherit via `extends`
- **Endpoint drift detection** — compare your collection against source code to find new, stale, or changed endpoints
- **Declarative tests** — YAML-based test assertions with CLI runner for CI integration
- Monaco editor for request body with JSON syntax highlighting
- Response viewer with body, headers, status code, and timing
- Request history persisted locally as JSONL
- CLI tool for scripting and CI workflows

## File Format

Each request is a single `.wire.yaml` file:

```yaml
name: Create User
method: POST
url: "{{base_url}}/api/users"
headers:
  Content-Type: application/json
  Authorization: "Bearer {{token}}"
body:
  type: json
  content:
    name: Jon
    email: jon@example.com
params:
  include: profile
```

Collections are organized as folder trees:

```
.wire/
├── wire.yaml              # collection metadata
├── envs/
│   ├── dev.yaml
│   └── prod.yaml
├── templates/
│   └── authenticated.wire.yaml
└── requests/
    ├── auth/
    │   └── login.wire.yaml
    └── users/
        ├── list.wire.yaml
        └── create.wire.yaml
```

## Templates

Requests can inherit from templates via `extends`:

```yaml
name: Get Users
method: GET
url: "{{base_url}}/api/users"
extends: authenticated
```

Templates live in `.wire/templates/` and use the same format. Headers and params merge additively (request wins on conflict). Body does a top-level JSON merge. Tests concatenate. Templates can chain up to 3 levels deep.

Collections can set default templates that apply to all requests:

```yaml
# wire.yaml
name: My API
version: 1
default_templates:
  - json-api
  - authenticated
```

## Environments & Secret Injection

Environment files define variables per environment:

```yaml
name: Development
variables:
  base_url: http://localhost:3000
  token: dev-token-123
```

### Response Snapshots (Golden File Testing)

Save API responses as golden file snapshots and detect regressions by diffing future responses against them.

```bash
# Save a snapshot
wire send .wire/requests/users/list.wire.yaml --snapshot -d .wire

# Test against saved snapshot (exit 1 if differences)
wire test .wire/requests/users/list.wire.yaml --snapshot -d .wire

# Update snapshot with current response
wire snapshot update .wire/requests/users/list.wire.yaml -d .wire
```

Snapshots are stored as canonical JSON in `.wire/snapshots/`, mirroring the request directory structure. Configure per-request ignore rules for dynamic fields:

```yaml
name: List Users
method: GET
url: "{{base_url}}/api/users"
snapshot:
  ignore:
    - body.timestamp
    - body.users[*].last_login
    - body.request_id
```

The structural JSON diff engine reports added, removed, and changed fields with human-readable paths (e.g. `body.users[0].name: "Alice" → "Bob"`). Status code and content-type header changes are also detected.

### Secret References

Instead of storing plaintext secrets, reference them from external sources using `$` prefixes:

```yaml
name: Production
variables:
  base_url: https://api.example.com
  api_key: $env:API_KEY
  db_password: $dotenv:DB_PASSWORD
  stripe_key: $aws:prod/stripe#secret_key
  vault_token: $vault:secret/data/app#token
```

Environment files with secret references are safe to commit to git — the actual values are resolved at request time.

| Prefix | Source | Setup |
|--------|--------|-------|
| `$env:VAR` | Process environment variable | `export VAR=value` in your shell or CI |
| `$dotenv:KEY` | `.env` file in project root | Add `KEY=value` to `.env` (gitignored) |
| `$aws:name#field` | AWS Secrets Manager | Install `aws` CLI, run `aws configure` |
| `$vault:path#field` | HashiCorp Vault | Install `vault` CLI, run `vault login` |

Validate all secret references resolve correctly:

```bash
wire env check -d .wire
```

In the GUI, secret values are masked by default. Use the lock/unlock toggle in the toolbar to reveal them for the current session.

## Request Chaining

Define multi-step API flows where responses feed into subsequent requests:

```yaml
name: Auth Flow
method: GET
url: "{{base_url}}/status"
chain:
  - run: auth/login
    extract:
      token: body.token
      user_id: body.user.id
  - run: users/profile
    extract:
      avatar_url: body.avatar
  - run: users/update-avatar
```

Each step executes sequentially. Extracted variables (`token`, `user_id`, `avatar_url`) are available to all subsequent steps via `{{variable}}` syntax. The chain halts on the first non-2xx response.

Extraction supports three sources:
- `body.field.path` — JSON response body (with array indexing: `body.items[0].id`)
- `headers.header-name` — response headers (case-insensitive)
- `status` — HTTP status code

Run chains from the CLI:

```bash
wire chain run .wire/requests/chains/auth-flow.wire.yaml -d .wire -e dev
```

In the GUI, click a request with a chain section to see the step list, then click **Run Chain** to execute. Each step is expandable to show full request/response details.

## Drift Detection

Compare your collection against source code to find endpoints that are new, stale, or changed:

```bash
wire drift ./src .wire               # detect drift
wire drift ./src .wire --fix         # auto-sync collection to match code
```

In the GUI, use the **Drift** tab in the sidebar to check drift for any collection that was generated from a codebase.

## Generate from Codebase

Scan a project's source code and generate a complete `.wire` collection:

```bash
wire generate ./my-express-app                    # creates .wire/ in project dir
wire generate ./my-project -o ./output-dir        # custom output directory
```

Supports:
- **ASP.NET** — controllers (`[HttpGet]`, `[HttpPost]`, etc.) and minimal APIs (`app.MapGet()`)
- **Express/Node** — `router.get()`, `app.post()`, chained routes

Discovered endpoints are grouped into subfolders by controller name (ASP.NET) or router filename (Express).

## Declarative Tests

Add test assertions to any request:

```yaml
name: Get User
method: GET
url: "{{base_url}}/api/users/1"
tests:
  - field: status
    equals: 200
  - field: body.name
    contains: "Jon"
  - field: body.email
    exists: true
  - field: elapsed_ms
    less_than: 500
```

Available operators: `equals`, `not_equals`, `contains`, `starts_with`, `ends_with`, `less_than`, `greater_than`, `is_array`, `is_object`, `is_string`, `is_number`, `exists`, `body_contains`, `body_matches` (regex).

Run tests from the CLI:

```bash
wire test .wire/requests/ -d .wire -e dev          # test all requests
wire test .wire/requests/auth/login.wire.yaml      # test a single request
wire test .wire/requests/ -o json                  # JSON output for CI
```

## Architecture

Cargo workspace with three crates:

| Crate | Purpose |
|-------|---------|
| `wire-core` | Shared library: HTTP execution, YAML parsing, variable interpolation, secret injection, chain execution, drift detection, codebase scanning, test runner |
| `wire-cli` | CLI binary consuming wire-core |
| `wire-app` | Tauri GUI consuming wire-core |

The React frontend lives in `ui/` and communicates with Rust via Tauri IPC.

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) (v20+)
- Tauri prerequisites: see [Tauri v2 docs](https://v2.tauri.app/start/prerequisites/)

### Development

```bash
# Install frontend dependencies
cd ui && npm install && cd ..

# Run the Tauri dev server (launches GUI with hot reload)
cargo tauri dev
```

### Install the CLI

```bash
cargo install --path crates/wire-cli
```

This installs `wire` globally. You can then use it from any directory.

### CLI Commands

```bash
wire send <file> -d .wire -e dev      # send a request
wire send <file> --snapshot -d .wire  # send and save response as snapshot
wire list .wire                        # list collection contents
wire test <path> -d .wire -e dev      # run test assertions
wire test <path> --snapshot -d .wire  # test + diff against saved snapshot
wire chain run <file> -d .wire        # execute a request chain
wire snapshot update <file> -d .wire  # overwrite snapshot with current response
wire generate <project_dir>           # generate collection from source code
wire drift <project_dir> .wire        # detect endpoint drift
wire drift <project_dir> .wire --fix  # auto-fix drift
wire env check -d .wire               # validate secret references
wire template list .wire              # list available templates
wire history                           # view request history
wire history clear                     # clear history
wire install-claude-skill               # install Claude Code integration
wire uninstall-claude-skill            # remove Claude Code integration
```

### Tests

```bash
# Rust tests (wire-core + wire-cli integration)
cargo test --workspace

# Frontend lint
cd ui && npm run lint
```

### Pre-commit Hooks

The repo includes pre-commit hooks for `cargo fmt`, `cargo clippy`, and `eslint`:

```bash
git config core.hooksPath .githooks
```

## License

AGPL-3.0 — see [LICENSE](LICENSE) for details.
