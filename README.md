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
- Create new collections from the GUI (names them, sets up directory structure)
- Save requests as `.wire.yaml` files — into a collection or standalone
- Multiple collections displayed as expandable accordions with color-coded HTTP method badges
- Collections dropdown menu for creating, importing, and managing collections
- Environment variables with `{{variable}}` interpolation and scoping (Global > Environment > Collection > Request)
- Environment switching (dev/staging/prod)
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
└── requests/
    ├── auth/
    │   └── login.wire.yaml
    └── users/
        ├── list.wire.yaml
        └── create.wire.yaml
```

Environment files define variables per environment:

```yaml
name: Development
variables:
  base_url: http://localhost:3000
  token: dev-token-123
```

## Architecture

Cargo workspace with three crates:

| Crate | Purpose |
|-------|---------|
| `wire-core` | Shared library: HTTP execution, YAML parsing, variable interpolation, history |
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

### CLI

```bash
# Build the CLI
cargo build -p wire-cli

# Send a request
wire send .wire/requests/auth/login.wire.yaml -d .wire -e dev

# List collection contents
wire list .wire

# View request history
wire history

# Clear history
wire history clear
```

### Tests

```bash
# Rust tests (wire-core + wire-cli integration)
cargo test --workspace

# Frontend tests
cd ui && npm test
```

### Pre-commit Hooks

The repo includes pre-commit hooks for `cargo fmt`, `cargo clippy`, and `eslint`:

```bash
git config core.hooksPath .githooks
```

## License

MIT
