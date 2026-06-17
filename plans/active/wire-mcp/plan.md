# wire mcp serve — collection as MCP tools (wire-h4a, WEDGE)

## Intent
Expose the collection over the Model Context Protocol (stdio JSON-RPC) so any
MCP client (Claude Desktop, Cursor, etc.) can use the API with the collection as
ground truth instead of guessing URLs. Concrete realization of wire-pxh.

## v1 scope (safe, no-network, fully testable)
Tools (all read/contract-oriented, reuse existing core):
- `list_endpoints` — the API surface (name, method, route, response fields). "Load the API into the agent."
- `get_request` — full definition of one request by name.
- `mock_response` — what an endpoint returns (uses `mock::resolve`), no network.
- `check_breaking` — contract changes vs the saved baseline (`breaking::compare`).

Protocol: JSON-RPC 2.0, newline-delimited over stdio. Handle `initialize`,
`tools/list`, `tools/call`; ignore notifications. stdout is the protocol channel
(logs go to stderr).

## Structure
- `wire-cli/src/mcp.rs`: a `Server` holding the loaded collection + wire_dir; a
  pure `handle(&Value) -> Option<Value>` dispatch (unit-tested) + tool fns; a
  thin stdio loop in `cmd_mcp`.
- CLI: `wire mcp [--dir]` (serve over stdio).

## Acceptance (v1)
- [ ] `initialize` returns protocolVersion + tools capability + serverInfo.
- [ ] `tools/list` returns the 4 tools with JSON-Schema inputs.
- [ ] `tools/call` for list_endpoints / mock_response / check_breaking returns text content.
- [ ] Unit tests on handle() for the handshake + a no-network tool call.
- [ ] Smoke test: pipe JSON-RPC lines into `wire mcp`, assert valid responses.

## Follow-ups
Live `send_request` + `run_test` (async/network); `check_drift`; resources/prompts.
