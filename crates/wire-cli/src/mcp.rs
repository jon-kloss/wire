//! Minimal Model Context Protocol (MCP) server exposing a Wire collection as
//! tools over stdio (newline-delimited JSON-RPC 2.0). Lets any MCP client use
//! the API with the collection as ground truth instead of guessing URLs.
//!
//! v1 tools are read/contract-oriented and do no network I/O, so the whole
//! dispatch is pure and unit-testable. stdout is the protocol channel — all
//! logging must go to stderr.

use serde_json::{json, Value};
use std::path::PathBuf;
use wire_core::collection::LoadedCollection;

const PROTOCOL_VERSION: &str = "2024-11-05";

pub struct Server {
    collection: LoadedCollection,
    wire_dir: PathBuf,
}

impl Server {
    pub fn new(collection: LoadedCollection, wire_dir: PathBuf) -> Self {
        Self {
            collection,
            wire_dir,
        }
    }

    pub fn endpoint_count(&self) -> usize {
        self.collection.requests.len()
    }

    /// Handle one JSON-RPC message; returns the response, or `None` for
    /// notifications (no `id`).
    pub fn handle(&self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");

        // Notifications (no id) get no response.
        id.as_ref()?;
        let id = id.unwrap();

        match method {
            "initialize" => Some(ok(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "wire", "version": env!("CARGO_PKG_VERSION") }
                }),
            )),
            "tools/list" => Some(ok(id, json!({ "tools": tool_specs() }))),
            "tools/call" => {
                let params = msg.get("params").cloned().unwrap_or(json!({}));
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                match self.call_tool(name, &args) {
                    Ok(text) => Some(ok(
                        id,
                        json!({ "content": [{ "type": "text", "text": text }] }),
                    )),
                    Err(text) => Some(ok(
                        id,
                        json!({ "content": [{ "type": "text", "text": text }], "isError": true }),
                    )),
                }
            }
            "ping" => Some(ok(id, json!({}))),
            _ => Some(err(id, -32601, &format!("method not found: {method}"))),
        }
    }

    fn call_tool(&self, name: &str, args: &Value) -> Result<String, String> {
        match name {
            "list_endpoints" => Ok(self.list_endpoints()),
            "get_request" => {
                let req_name = args
                    .get("name")
                    .and_then(|n| n.as_str())
                    .ok_or("missing required arg: name")?;
                self.get_request(req_name)
            }
            "mock_response" => {
                let method = args
                    .get("method")
                    .and_then(|m| m.as_str())
                    .ok_or("missing required arg: method")?;
                let path = args
                    .get("path")
                    .and_then(|p| p.as_str())
                    .ok_or("missing required arg: path")?;
                Ok(self.mock_response(method, path))
            }
            "check_breaking" => self.check_breaking(),
            _ => Err(format!("unknown tool: {name}")),
        }
    }

    fn list_endpoints(&self) -> String {
        let endpoints: Vec<Value> = self
            .collection
            .requests
            .iter()
            .map(|(_, r)| {
                json!({
                    "name": r.name,
                    "method": r.method.to_uppercase(),
                    "route": wire_core::drift::normalize_route(&r.url),
                    "url": r.url,
                    "response_fields": r.response_schema.iter().map(|(n, _)| n).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({ "collection": self.collection.metadata.name, "endpoints": endpoints }).to_string()
    }

    fn get_request(&self, name: &str) -> Result<String, String> {
        self.collection
            .requests
            .iter()
            .find(|(_, r)| r.name.eq_ignore_ascii_case(name))
            .map(|(_, r)| serde_json::to_string_pretty(r).unwrap_or_else(|_| "{}".to_string()))
            .ok_or_else(|| format!("no request named '{name}' in the collection"))
    }

    fn mock_response(&self, method: &str, path: &str) -> String {
        match wire_core::mock::resolve(&self.collection.requests, &self.wire_dir, method, path) {
            Some(m) => json!({
                "status": m.status,
                "content_type": m.content_type,
                "body": m.body,
            })
            .to_string(),
            None => json!({ "error": "no matching endpoint in collection" }).to_string(),
        }
    }

    fn check_breaking(&self) -> Result<String, String> {
        match wire_core::breaking::compare(&self.wire_dir) {
            Ok(report) => {
                Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()))
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn err(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_specs() -> Vec<Value> {
    vec![
        json!({
            "name": "list_endpoints",
            "description": "List every endpoint in the Wire collection (name, HTTP method, route, and response field names). Use this to load the API surface before making requests.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "get_request",
            "description": "Get the full definition (method, url, headers, params, body, tests) of one request by its name.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": { "type": "string", "description": "The request's name." } },
                "required": ["name"]
            }
        }),
        json!({
            "name": "mock_response",
            "description": "Return the contract-accurate mock response (status + body) Wire would serve for a given HTTP method and path, from saved snapshots or the response schema. No network call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "method": { "type": "string", "description": "HTTP method, e.g. GET." },
                    "path": { "type": "string", "description": "Request path, e.g. /pets/42." }
                },
                "required": ["method", "path"]
            }
        }),
        json!({
            "name": "check_breaking",
            "description": "Compare the current collection contract against the saved baseline and report BREAKING / WARNING / INFO changes. Requires a baseline (wire breaking --save).",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use wire_core::collection::{WireCollection, WireRequest};

    fn server() -> Server {
        let req = WireRequest {
            name: "List Pets".into(),
            method: "GET".into(),
            url: "{{base_url}}/pets".into(),
            headers: HashMap::new(),
            params: HashMap::new(),
            body: None,
            extends: None,
            tests: Vec::new(),
            response_schema: vec![("name".into(), "string".into())],
            chain: Vec::new(),
            snapshot: None,
        };
        let collection = LoadedCollection {
            metadata: WireCollection {
                name: "Petstore".into(),
                version: 1,
                active_env: None,
                default_template: None,
                default_templates: Vec::new(),
                source_dir: None,
            },
            requests: vec![(PathBuf::from(".wire/requests/pets/list.wire.yaml"), req)],
            environments: HashMap::new(),
        };
        Server::new(collection, PathBuf::from(".wire"))
    }

    #[test]
    fn initialize_advertises_tools() {
        let s = server();
        let resp = s
            .handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
            .unwrap();
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert!(resp["result"]["capabilities"]["tools"].is_object());
        assert_eq!(resp["result"]["serverInfo"]["name"], "wire");
    }

    #[test]
    fn notifications_get_no_response() {
        let s = server();
        assert!(s
            .handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none());
    }

    #[test]
    fn tools_list_returns_specs() {
        let s = server();
        let resp = s
            .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"list_endpoints"));
        assert!(names.contains(&"mock_response"));
    }

    #[test]
    fn tools_call_list_endpoints() {
        let s = server();
        let resp = s
            .handle(&json!({
                "jsonrpc":"2.0","id":3,"method":"tools/call",
                "params": { "name": "list_endpoints", "arguments": {} }
            }))
            .unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let v: Value = serde_json::from_str(text).unwrap();
        assert_eq!(v["collection"], "Petstore");
        assert_eq!(v["endpoints"][0]["route"], "/pets");
        assert_eq!(v["endpoints"][0]["method"], "GET");
    }

    #[test]
    fn tools_call_mock_response() {
        let s = server();
        let resp = s
            .handle(&json!({
                "jsonrpc":"2.0","id":4,"method":"tools/call",
                "params": { "name": "mock_response", "arguments": { "method": "GET", "path": "/pets" } }
            }))
            .unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let v: Value = serde_json::from_str(text).unwrap();
        assert_eq!(v["status"], 200);
        // schema-shaped body
        let body: Value = serde_json::from_str(v["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["name"], "string");
    }

    #[test]
    fn unknown_method_errors() {
        let s = server();
        let resp = s
            .handle(&json!({"jsonrpc":"2.0","id":5,"method":"frobnicate"}))
            .unwrap();
        assert_eq!(resp["error"]["code"], -32601);
    }
}
