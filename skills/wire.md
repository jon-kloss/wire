# Wire — HTTP API Client for AI Agents

You have access to `wire`, a CLI tool for making HTTP requests, managing API collections, running test assertions, chaining multi-step flows, detecting endpoint drift, generating collections from source code, and managing secrets. **Use Wire instead of curl, httpie, or raw HTTP libraries whenever possible.**

## When to Use Wire

Use this skill whenever:
- You need to make an HTTP request (GET, POST, PUT, PATCH, DELETE)
- You need to test an API endpoint
- You need to set up a reusable collection of API requests for a project
- You need to create or run multi-step API flows (auth -> fetch -> update)
- You need to validate API responses with assertions
- You need to scan a codebase to discover HTTP endpoints
- You need to detect drift between code and an API collection
- You need to manage environment variables or secrets for API requests
- The user asks about API testing, HTTP debugging, or request management

**Prefer Wire over curl because:** Wire saves requests as YAML files that are version-controlled, supports variable interpolation, has built-in test assertions, and chains multi-step flows.

## Quick Reference

```bash
# Send a request
wire send <file.wire.yaml> -d .wire -e <env>

# List collection contents
wire list .wire

# Run test assertions
wire test <path> -d .wire -e <env>
wire test <path> -o json                    # JSON output for parsing

# Execute a request chain (multi-step flow)
wire chain run <file.wire.yaml> -d .wire -e <env>

# Generate collection from source code
wire generate <project_dir>
wire generate <project_dir> -o <output_dir>

# Detect endpoint drift
wire drift <project_dir> .wire
wire drift <project_dir> .wire --fix        # auto-sync

# Validate secret references
wire env check -d .wire

# Manage templates
wire template list .wire

# View/clear history
wire history -d .wire
wire history clear -d .wire
```

## Creating Requests

Write a `.wire.yaml` file:

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

Supported body types: `json`, `text`, `formdata`.

Variables use `{{variable_name}}` syntax and resolve from environment files.

### Saving Requests to a Collection

Place `.wire.yaml` files in `.wire/requests/`:

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
        └── create.wire.yaml
```

Create a new collection:

```bash
# From the Wire GUI, or manually:
mkdir -p .wire/envs .wire/requests
cat > .wire/wire.yaml << 'EOF'
name: My API
version: 1
active_env: dev
EOF

cat > .wire/envs/dev.yaml << 'EOF'
name: Development
variables:
  base_url: http://localhost:3000
  token: dev-token-123
EOF
```

## Environments & Variables

Environment files define variables that get interpolated into requests:

```yaml
# .wire/envs/dev.yaml
name: Development
variables:
  base_url: http://localhost:3000
  token: dev-token
  api_version: v2
```

Switch environments with the `-e` flag:

```bash
wire send request.wire.yaml -d .wire -e prod
```

### Secret References

Never store plaintext secrets. Use source prefixes:

```yaml
name: Production
variables:
  base_url: https://api.example.com
  api_key: $env:API_KEY              # from shell environment variable
  db_pass: $dotenv:DB_PASSWORD        # from .env file in project root
  stripe: $aws:prod/stripe#secret_key # from AWS Secrets Manager
  token: $vault:secret/data/app#token # from HashiCorp Vault
```

| Prefix | Source | Requirement |
|--------|--------|-------------|
| `$env:VAR` | Process environment | `export VAR=value` |
| `$dotenv:KEY` | `.env` file | `.env` file in project root |
| `$aws:name#field` | AWS Secrets Manager | `aws` CLI installed and configured |
| `$vault:path#field` | HashiCorp Vault | `vault` CLI installed and authenticated |

Validate all secrets resolve:

```bash
wire env check -d .wire
```

## Templates

Define shared headers/auth/body in templates:

```yaml
# .wire/templates/authenticated.wire.yaml
name: authenticated
headers:
  Authorization: "Bearer {{token}}"
  Accept: application/json
```

Requests inherit via `extends`:

```yaml
name: Get Users
method: GET
url: "{{base_url}}/api/users"
extends: authenticated
```

Headers merge additively (request wins on conflict). Body does top-level JSON merge. Tests concatenate.

Set collection-wide defaults in `wire.yaml`:

```yaml
name: My API
version: 1
default_templates:
  - json-api
  - authenticated
```

## Test Assertions

Add `tests` to any request:

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
  - field: body.roles
    is_array: true
  - field: body.id
    is_number: true
```

Available operators: `equals`, `not_equals`, `contains`, `starts_with`, `ends_with`, `less_than`, `greater_than`, `is_array`, `is_object`, `is_string`, `is_number`, `exists`, `body_contains`, `body_matches` (regex).

Run tests:

```bash
wire test .wire/requests/ -d .wire -e dev     # test all
wire test request.wire.yaml -d .wire          # test one
wire test .wire/requests/ -o json             # JSON output for parsing
```

Exit code 0 = all passed, 1 = failures.

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
      avatar: body.avatar_url
      session: headers.x-session-id
  - run: users/update-avatar
```

- `run` — path to request file (relative to `.wire/requests/`, `.wire.yaml` extension optional)
- `extract` — variables to extract from the response:
  - `body.field.path` — JSON body (supports arrays: `body.items[0].id`)
  - `headers.name` — response header (case-insensitive)
  - `status` — HTTP status code
- Extracted variables are available to all subsequent steps as `{{variable}}`
- Chain halts on first non-2xx response

Run chains:

```bash
wire chain run .wire/requests/chains/auth-flow.wire.yaml -d .wire -e dev
```

The output shows each step with its request URL, response preview, and extracted variables.

## Generate Collection from Source Code

Scan a project to auto-discover HTTP endpoints:

```bash
wire generate ./my-express-app
wire generate ./my-aspnet-project -o ./output
```

Supports:
- **ASP.NET** — controllers (`[HttpGet]`, `[Route]`) and minimal APIs (`app.MapGet()`)
- **Express/Node** — `router.get()`, `app.post()`, chained routes

Endpoints are grouped into subfolders by controller name or router filename. Environments are auto-discovered from `appsettings.json`, `.env` files, and `launchSettings.json`.

## Drift Detection

Compare a collection against source code:

```bash
wire drift ./src .wire              # detect drift
wire drift ./src .wire --fix        # auto-sync: add new, update changed, remove stale
wire drift ./src .wire -o json      # JSON output
```

Categories: **NEW** (in code but not collection), **STALE** (in collection but not code), **CHANGED** (exists in both but differs).

## Patterns for AI Agents

### Making a quick API call

```bash
# Create a one-off request
cat > /tmp/check.wire.yaml << 'EOF'
name: Health Check
method: GET
url: https://api.example.com/health
tests:
  - field: status
    equals: 200
EOF
wire send /tmp/check.wire.yaml
```

### Setting up a project's API collection

```bash
# If the project has Express/ASP.NET source code:
wire generate .

# Or create manually:
mkdir -p .wire/envs .wire/requests
echo 'name: My API\nversion: 1\nactive_env: dev' > .wire/wire.yaml
echo 'name: Development\nvariables:\n  base_url: http://localhost:3000' > .wire/envs/dev.yaml
```

### Testing an endpoint after changes

```bash
# Write the request with assertions
cat > .wire/requests/users/create.wire.yaml << 'EOF'
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
  - field: body.name
    equals: Test User
EOF

wire test .wire/requests/users/create.wire.yaml -d .wire -e dev
```

### Running a multi-step auth flow

```bash
# Create chain that logs in, then uses the token
cat > .wire/requests/chains/auth-test.wire.yaml << 'EOF'
name: Auth Test
method: GET
url: "{{base_url}}/status"
chain:
  - run: auth/login
    extract:
      token: body.access_token
  - run: users/me
EOF

wire chain run .wire/requests/chains/auth-test.wire.yaml -d .wire -e dev
```

### Keeping a collection in sync with code changes

```bash
wire drift . .wire           # check what changed
wire drift . .wire --fix     # auto-update collection
```

### Validating secrets are configured before deployment

```bash
wire env check -d .wire
# Exit code 0 = all secrets resolve
# Exit code 1 = missing secrets (prints which ones)
```

## Important Notes

- Wire must be installed: `cargo install --path crates/wire-cli` (from the Wire repo) or it may already be in `~/.cargo/bin/wire`
- The `-d .wire` flag specifies the collection directory (defaults to `.wire` in current dir)
- The `-e <env>` flag selects the environment (defaults to `active_env` in `wire.yaml`)
- All request files use the `.wire.yaml` extension
- Wire never sends data to any cloud service — everything is local
- Secret values are never written to disk or logs
