# Wire — HTTP API Client for AI Agents

You have access to `wire`, a CLI tool for HTTP requests, API testing, request chaining, drift detection, codebase scanning, and secret management. **Use Wire instead of curl, httpie, or raw HTTP libraries whenever possible.**

## CRITICAL: Exact Syntax Reference

**These are the ONLY valid Wire commands. Do NOT invent commands like `wire validate`, `wire check`, `wire sync`, or `wire spec`.**

> **Note:** `wire breaking` is also available — see the Breaking Change Detection section below.

```bash
wire send <file> -d .wire -e <env>           # send a request
wire send <file> --snapshot -d .wire         # send and save response as golden file
wire test <path> -d .wire -e <env>           # run test assertions
wire test <path> --snapshot -d .wire         # test + diff against saved snapshot
wire test <path> -d .wire -o json            # JSON output for CI
wire chain run <file> -d .wire -e <env>      # execute a chain
wire snapshot update <file> -d .wire         # overwrite snapshot with current response
wire generate <dir>                           # generate collection from code
wire generate <dir> -o <output>              # generate to custom output dir
wire drift <dir> -d .wire                    # detect endpoint drift
wire drift <dir> -d .wire --fix              # auto-fix drift (create/update/delete)
wire drift <dir> -d .wire -o json            # JSON drift output for CI
wire env check -d .wire                       # validate secret references
wire list .wire                               # list collection
wire template list .wire                      # list templates
wire history -d .wire                         # view history
wire history clear                            # clear all history
wire breaking --save -d .wire                 # save contract baseline snapshot
wire breaking -d .wire                        # detect breaking changes vs baseline
wire breaking -d .wire -o json               # JSON output for CI
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

## CRITICAL: Exact YAML Format Reference

**Do NOT invent field names. Use ONLY these exact fields and structures.**

**Request file format (every field shown):**
```yaml
name: Create User                    # REQUIRED: human-readable name
method: POST                         # REQUIRED: GET, POST, PUT, PATCH, DELETE
url: "{{base_url}}/api/users"       # REQUIRED: URL with {{variable}} interpolation
extends: authenticated               # optional: template name (NOT "template:")
headers:                             # optional: key-value pairs
  Content-Type: application/json
params:                              # optional: query parameters
  page: "1"
body:                                # optional: request body
  type: json                         #   REQUIRED if body: json, text, or form_data
  content:                           #   REQUIRED if body: the actual content
    name: Test User
    email: test@example.com
tests:                               # optional: test assertions (see reference below)
  - field: status                    #   REQUIRED per assertion: what to check
    equals: 201                      #   REQUIRED per assertion: operator + value
  - field: body.id
    is_number: true
  - field: body.name
    equals: Test User
snapshot:                            # optional: snapshot config
  ignore:                            #   fields to ignore in snapshot diff
    - body.timestamp
chain:                               # optional: multi-step flow
  - run: users/create               #   REQUIRED per step: short path to request
    extract:                         #   optional: extract values from response
      user_id: body.id
  - run: users/get-by-id
```

**WRONG field names (do NOT use these):**
- ❌ `template:` → use `extends:`
- ❌ `steps:` → use `chain:`
- ❌ `request:` or `file:` → use `run:`
- ❌ `variables:` → use `extract:`
- ❌ `assert:` or `expect:` → use `field:` + operator
- ❌ `body: { json: {} }` → use `body: { type: json, content: {} }`

**Exact directory structure. Do NOT deviate:**

```
.wire/
├── wire.yaml                          # REQUIRED: collection metadata
├── envs/                              # NOT "environments/"
│   ├── dev.yaml
│   └── prod.yaml
├── templates/                         # shared headers/auth
│   └── authenticated.wire.yaml
├── snapshots/                         # golden file snapshots (auto-created by --snapshot)
│   └── users/
│       └── list.snapshot.json
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
- **Source code exists?** Run `wire drift . -d .wire` to check for drift

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

### 5. ALWAYS Save Snapshots for Stable Endpoints

When an endpoint's response shape is stable and established, **save a snapshot** so future changes are caught automatically. Snapshots are golden files — they capture the exact response structure as a baseline.

**When to create snapshots:**
- After confirming an endpoint works correctly for the first time
- After a CRUD chain passes — snapshot the individual GET/list responses
- When an endpoint's response contract matters (public APIs, integration points)
- When onboarding to an existing project — snapshot current responses as a baseline

**How to create and use snapshots:**
```bash
# 1. Save a snapshot after verifying the endpoint works
wire send .wire/requests/users/list.wire.yaml --snapshot -d .wire

# 2. Later, test against the snapshot to detect drift
wire test .wire/requests/users/list.wire.yaml --snapshot -d .wire

# 3. If the response intentionally changed, update the snapshot
wire snapshot update .wire/requests/users/list.wire.yaml -d .wire
```

**Configure ignore rules** for dynamic fields that change between requests (timestamps, IDs, tokens). Add these in the request file's `snapshot` section:

```yaml
name: List Users
method: GET
url: "{{base_url}}/api/users"
snapshot:
  ignore:
    - body.timestamp            # exact path match
    - body.users[*].last_login  # wildcard — matches any array index
    - body.request_id
tests:
  - field: status
    equals: 200
```

**Do NOT snapshot:**
- Endpoints with fully dynamic responses (random data, timestamps everywhere)
- Endpoints you haven't verified work correctly yet — test first, snapshot after
- Binary responses (images, files) — snapshots are for JSON/text APIs

**Snapshot diff output** shows structural changes with human-readable paths:
```
~ body.users[0].name: "Alice" → "Bob"
+ body.users[0].role: "admin"
- body.deprecated_field: "old_value"

1 added, 1 removed, 1 changed
```

Exit code 0 = snapshot matches, exit code 1 = differences found (useful for CI).

### 6. ALWAYS Use Breaking Change Detection When Modifying APIs

When you modify, refactor, or remove any API endpoint in a project that has a `.wire/` collection, **always check for breaking changes** before committing.

**When to save a baseline:**
- After generating a new collection (`wire generate`)
- After confirming all tests pass on a stable API
- Before starting any refactoring work on endpoints
- At the start of a sprint or release cycle

**When to check for breaking changes:**
- After modifying any endpoint's response shape, parameters, or headers
- After renaming or removing fields from response models
- After adding required parameters to existing endpoints
- After removing endpoints
- Before creating a PR that touches API code

**How to use:**
```bash
# 1. Save baseline when the API is known-good
wire breaking --save -d .wire

# 2. After making changes, check what broke
wire breaking -d .wire
```

**Example output:**
```
BREAKING (2):
  ✗ GET /api/users — response field 'email' removed
  ✗ POST /api/orders — endpoint removed

WARNING (1):
  ⚠ GET /api/items — new required param 'tenant_id'

INFO (3):
  + POST /api/users/invite — new endpoint added
  + GET /api/users — new response field 'avatar_url'
  + GET /api/items — param 'sort' removed

Result: FAIL (2 breaking changes)
```

**Severity classification:**

| Severity | Exit Code | What triggers it |
|----------|-----------|------------------|
| BREAKING | 1 | Endpoint removed, response field removed, field type changed, body removed, body type changed |
| WARNING | 0 | New required param added, new required header added, body added to endpoint |
| INFO | 0 | New endpoint, new response field, param no longer required, header no longer required |

**CI integration:** Use `wire breaking -d .wire -o json` for structured output. Exit code 1 = breaking changes found (fail the build).

**The snapshot file** is saved at `.wire/contract-snapshot.json`. It captures the full structural definition of every endpoint: method, URL, params, headers, body schema, and response schema. Commit this file to version control so the baseline is shared across the team.

### 7. ALWAYS Use Secret References for Sensitive Values

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
8. **Save snapshots** for stable endpoints with `wire send <file> --snapshot -d .wire`
9. **Add ignore rules** for dynamic fields (timestamps, IDs) in the request's `snapshot` section
10. **Run `wire drift`** if source scanning is available
11. **Run `wire breaking`** to check for breaking changes against the baseline
12. **Update the baseline** with `wire breaking --save` after intentional API changes are verified

## Important Notes

- Wire must be installed: `cargo install --path crates/wire-cli` or may already be at `~/.cargo/bin/wire`
- The `-d .wire` flag specifies the collection directory (defaults to `.wire`)
- The `-e <env>` flag selects the environment (defaults to `active_env` in `wire.yaml`)
- All request files use the `.wire.yaml` extension
- Wire is local-only — no cloud, no accounts, no data leaves your machine
- Secret values are never written to disk or logs
- Run `wire install-claude-skill` to install this skill, `wire uninstall-claude-skill` to remove it
