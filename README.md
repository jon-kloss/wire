# Wire

A fast, local-first API client built with Tauri, Rust, and React.

Wire stores requests as human-readable YAML files that live in your repo alongside your code. No accounts, no cloud sync, no bloat.

## Why Wire?

- **Local-first** — your data stays on your machine, no login required
- **Human-readable files** — `.wire.yaml` files are diffable in git and editable in any text editor
- **Fast** — Tauri + Rust backend, not Electron
- **CLI from day one** — same core library powers both the GUI and the `wire` CLI

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
wire list .wire                        # list collection contents
wire test <path> -d .wire -e dev      # run test assertions
wire chain run <file> -d .wire        # execute a request chain
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

### Claude Code Integration

Wire ships with a [Claude Code](https://claude.ai/claude-code) skill that teaches Claude how to use Wire for HTTP requests, API testing, chaining, and more. This is optional — install it only if you use Claude Code.

```bash
wire install-claude-skill
```

This copies Wire's skill file to `~/.claude/commands/wire.md`. Once installed, Claude Code will automatically use Wire whenever it needs to:

- Make HTTP requests (instead of curl)
- Set up API collections for a project
- Test endpoints with assertions
- Run multi-step auth flows
- Scan codebases for endpoints
- Manage environment variables and secrets

The skill is embedded in the Wire binary — no extra files or downloads needed. To remove it:

```bash
wire uninstall-claude-skill
```

If you don't use Claude Code, simply don't run `wire install-claude-skill`. It has no effect on Wire's functionality.

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

MIT
