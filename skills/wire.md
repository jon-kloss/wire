# Wire — HTTP API Client for AI Agents

You have access to `wire`, a CLI tool for HTTP requests, API testing, request chaining, drift detection, codebase scanning, and secret management. **Use Wire instead of curl, httpie, or raw HTTP libraries whenever possible.**

## CRITICAL: Exact Syntax Reference

**These are the ONLY valid Wire commands. Do NOT invent commands like `wire validate`, `wire check`, `wire sync`, or `wire spec`.**

```bash
wire send <file> -d .wire -e <env>           # send a request
wire send <file> --snapshot -d .wire         # send and save response as golden file
wire test <path> -d .wire -e <env>           # run test assertions
wire test <path> --snapshot -d .wire         # test + diff against saved snapshot
wire chain run <file> -d .wire -e <env>      # execute a chain
wire snapshot update <file> -d .wire         # overwrite snapshot with current response
wire generate <dir>                           # generate collection from code
wire drift <dir> .wire [--fix]               # detect/fix endpoint drift
wire env check -d .wire                       # validate secret references
wire list .wire                               # list collection
wire template list .wire                      # list templates
wire history -d .wire                         # view history
```

**CRITICAL: File path resolution for `send` and `test` commands.**
These commands expect a file path relative to your current directory, NOT a short name.

```bash
# ✅ CORRECT — full relative path from project root
wire send .wire/requests/echo/echo.wire.yaml -d .wire
wire test .wire/requests/echo/echo.wire.yaml -d .wire

# ❌ WRONG — short names do NOT work with send/test
wire send echo/echo -d .wire
wire send requests/echo/echo -d .wire
wire send echo/echo.wire.yaml -d .wire
```

**Exception:** Chain steps use short names (resolved relative to `.wire/requests/`):
```yaml
chain:
  - run: users/create        # resolves to .wire/requests/users/create.wire.yaml
  - run: users/get-by-id     # short names ONLY work inside chain definitions
```

**Exact directory structure. Do NOT deviate:**

```
.wire/
├── wire.yaml                          # REQUIRED: collection metadata
├── envs/                              # NOT "environments/"
│   ├── dev.yaml
│   └── prod.yaml
├── templates/                         # shared headers/auth
│   └── authenticated.wire.yaml
└── requests/                          # ALL requests go here, in subfolders
    ├── auth/
    │   └── login.wire.yaml
    ├── users/
    │   ├── create.wire.yaml
    │   ├── list.wire.yaml
    │   ├── get-by-id.wire.yaml
    │   ├── update.wire.yaml
    │   └── delete.wire.yaml
    └── chains/
        └── user-crud.wire.yaml
```

**wire.yaml is REQUIRED:**

```yaml
name: My API
version: 1
active_env: dev
```

**Exact chain syntax. Do NOT use `steps`, `file`, or `variables` — use `chain`, `run`, `extract`:**

```yaml
name: User CRUD Flow
method: GET
url: "{{base_url}}/api/users"
chain:
  - run: users/create          # path relative to .wire/requests/, no extension needed
    extract:
      user_id: body.id         # extract from JSON response body
  - run: users/get-by-id       # {{user_id}} is now available in this request
  - run: users/update          # {{user_id}} still available
  - run: users/delete          # {{user_id}} still available
```

## Proactive Behavior — What You MUST Do Automatically

Wire is not just a tool you use when asked. You are responsible for keeping the project's API surface documented, tested, and verified. Do these things **without being asked**:

### 1. ALWAYS Write Tests for Every Endpoint

Every request file MUST have test assertions. No exceptions.

```yaml
name: Create User
method: POST
url: "{{base_url}}/api/users"
headers:
  Content-Type: application/json
body:
  type: json
  content:
    name: Test User
    email: test@example.com
tests:
  - field: status
    equals: 201
  - field: body.id
    exists: true
  - field: body.id
    is_number: true
  - field: body.name
    equals: Test User
  - field: body.email
    equals: test@example.com
```

**Minimum test coverage per request:**
- Status code assertion (always)
- Response body field existence for key fields
- Type assertions for critical fields (is_number, is_string, is_array)
- Value assertions where the expected value is known
- `elapsed_ms less_than` for performance-sensitive endpoints

**BAD tests (too weak, do NOT write these):**
```yaml
tests:
  - field: body
    exists: true        # tautological — body always exists on 200
  - field: status
    greater_than: 0     # useless — status is always > 0
```

### 2. ALWAYS Chain CRUD Operations

When a resource has Create, Read, Update, and Delete endpoints, **always create a chain** that tests the full lifecycle:

```yaml
name: User CRUD Flow
method: GET
url: "{{base_url}}/api/users"
chain:
  - run: users/create
    extract:
      user_id: body.id
      user_name: body.name
  - run: users/get-by-id
    extract:
      fetched_name: body.name
  - run: users/update
    extract:
      updated_name: body.name
  - run: users/delete
```

**When to create chains:**
- Any CRUD resource (create -> read -> update -> delete)
- Auth flows (login -> use token -> refresh -> use new token)
- Multi-step workflows (create order -> add items -> checkout -> verify)
- Any flow where step N depends on step N-1's response

### 3. ALWAYS Generate and Maintain Collections

When you encounter a project with HTTP endpoints:

- **No `.wire/` directory?** Run `wire generate .` or create one manually with `wire.yaml` + `envs/` + `requests/`
- **New endpoint added?** Create a `.wire.yaml` request file in the appropriate subfolder with tests
- **Existing endpoints lack request files?** Create them — don't leave gaps
- **Endpoint changed?** Update the request file and its tests
- **Endpoint removed?** Delete the request file
- **Source code exists?** Run `wire drift . .wire` to check for drift

### 4. ALWAYS Use Templates for Shared Patterns

If multiple requests share headers or auth, create a template:

```yaml
# .wire/templates/authenticated.wire.yaml
name: authenticated
headers:
  Authorization: "Bearer {{token}}"
  Accept: application/json
  Content-Type: application/json
```

Then every request uses `extends: authenticated` instead of repeating headers. Set collection defaults in `wire.yaml`:

```yaml
name: My API
version: 1
active_env: dev
default_templates:
  - authenticated
```

### 5. ALWAYS Use Secret References for Sensitive Values

Never put real tokens, passwords, or API keys in environment files:

```yaml
# .wire/envs/prod.yaml
name: Production
variables:
  base_url: https://api.example.com
  api_key: $env:API_KEY
  db_pass: $dotenv:DB_PASSWORD
  stripe: $aws:prod/stripe#secret_key
  token: $vault:secret/data/app#token
```

| Prefix | Source | Requirement |
|--------|--------|-------------|
| `$env:VAR` | Process environment | `export VAR=value` |
| `$dotenv:KEY` | `.env` file | `.env` in project root |
| `$aws:name#field` | AWS Secrets Manager | `aws` CLI configured |
| `$vault:path#field` | HashiCorp Vault | `vault` CLI authenticated |

## Test Assertion Reference

| Operator | Example | What it checks |
|----------|---------|----------------|
| `equals` | `equals: 200` | Exact match |
| `not_equals` | `not_equals: 500` | Not equal |
| `contains` | `contains: "Jon"` | Substring |
| `starts_with` | `starts_with: "Bearer"` | Prefix |
| `ends_with` | `ends_with: ".json"` | Suffix |
| `less_than` | `less_than: 500` | Numeric < |
| `greater_than` | `greater_than: 0` | Numeric > |
| `is_array` | `is_array: true` | Array type |
| `is_object` | `is_object: true` | Object type |
| `is_string` | `is_string: true` | String type |
| `is_number` | `is_number: true` | Number type |
| `exists` | `exists: true` | Field exists |
| `body_contains` | `body_contains: "ok"` | Raw body substring |
| `body_matches` | `body_matches: "id.*[0-9]+"` | Regex on body |

Fields: `status`, `elapsed_ms`, `body.<json.path>`, `body.<path>[0].field`, `header.<name>`.

## Chain Extraction Reference

Each chain step can extract values from the response:

- `body.field.path` — JSON body (supports arrays: `body.items[0].id`)
- `headers.header-name` — response header (case-insensitive)
- `status` — HTTP status code

Extracted variables become `{{variable_name}}` in all subsequent steps. Chain halts on first non-2xx response.

## Standard Workflow When Building/Modifying Endpoints

Follow this every time:

1. **Write the endpoint code** (the user's request)
2. **Create/update `.wire/requests/<resource>/<action>.wire.yaml`** with method, URL, headers, body
3. **Add test assertions** — status code, response fields, types
4. **If CRUD resource**, create/update the chain in `.wire/requests/chains/`
5. **If shared auth/headers**, create a template in `.wire/templates/`
6. **Run `wire test`** to verify
7. **Run `wire chain run`** if chains exist
8. **Run `wire drift`** if source scanning is available

## Important Notes

- Wire must be installed: `cargo install --path crates/wire-cli` or may already be at `~/.cargo/bin/wire`
- The `-d .wire` flag specifies the collection directory (defaults to `.wire`)
- The `-e <env>` flag selects the environment (defaults to `active_env` in `wire.yaml`)
- All request files use the `.wire.yaml` extension
- Wire is local-only — no cloud, no accounts, no data leaves your machine
- Secret values are never written to disk or logs
- Run `wire install-claude-skill` to install this skill, `wire uninstall-claude-skill` to remove it
