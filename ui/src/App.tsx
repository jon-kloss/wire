import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { IpcResponse } from "./types";
import "./App.css";

function App() {
  const [method, setMethod] = useState("GET");
  const [url, setUrl] = useState("");
  const [headersText, setHeadersText] = useState("");
  const [bodyText, setBodyText] = useState("");
  const [activeTab, setActiveTab] = useState<"params" | "headers" | "body">(
    "headers"
  );
  const [response, setResponse] = useState<IpcResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [responseTab, setResponseTab] = useState<"body" | "headers">("body");

  const handleSend = async () => {
    if (!url.trim()) return;

    setLoading(true);
    setError(null);
    setResponse(null);

    try {
      // Build the request object to send via Tauri
      const headers: Record<string, string> = {};
      if (headersText.trim()) {
        for (const line of headersText.split("\n")) {
          const idx = line.indexOf(":");
          if (idx > 0) {
            headers[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
          }
        }
      }

      let body = undefined;
      if (bodyText.trim() && ["POST", "PUT", "PATCH"].includes(method)) {
        try {
          body = {
            type: "json",
            content: JSON.parse(bodyText),
          };
        } catch {
          body = {
            type: "text",
            content: bodyText,
          };
        }
      }

      const request = {
        name: "Quick Request",
        method,
        url,
        headers,
        params: {},
        body: body ?? null,
      };

      // Save to a temp file and send via backend
      const tempPath = `/tmp/wire-quick-request.wire.yaml`;
      await invoke("save_request", { path: tempPath, request });
      const result = await invoke<IpcResponse>("send_request", {
        file: tempPath,
        env: null,
      });

      setResponse(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const formatBody = (body: string): string => {
    try {
      return JSON.stringify(JSON.parse(body), null, 2);
    } catch {
      return body;
    }
  };

  const statusColor = (status: number): string => {
    if (status < 300) return "#4ec9b0";
    if (status < 400) return "#dcdcaa";
    return "#f44747";
  };

  return (
    <div className="app">
      {/* Left Panel: Collection Tree */}
      <aside className="sidebar">
        <div className="panel-header">
          <h2>Collections</h2>
        </div>
        <div className="panel-body">
          <p className="placeholder">No collections loaded</p>
        </div>
      </aside>

      {/* Center Panel: Request Builder */}
      <main className="request-builder">
        <div className="url-bar">
          <select
            className="method-select"
            value={method}
            onChange={(e) => setMethod(e.target.value)}
          >
            <option value="GET">GET</option>
            <option value="POST">POST</option>
            <option value="PUT">PUT</option>
            <option value="PATCH">PATCH</option>
            <option value="DELETE">DELETE</option>
          </select>
          <input
            className="url-input"
            type="text"
            placeholder="Enter request URL..."
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSend()}
          />
          <button className="send-btn" onClick={handleSend} disabled={loading}>
            {loading ? "Sending..." : "Send"}
          </button>
        </div>
        <div className="request-tabs">
          <div className="tabs">
            <button
              className={`tab ${activeTab === "headers" ? "active" : ""}`}
              onClick={() => setActiveTab("headers")}
            >
              Headers
            </button>
            <button
              className={`tab ${activeTab === "body" ? "active" : ""}`}
              onClick={() => setActiveTab("body")}
            >
              Body
            </button>
          </div>
          <div className="tab-content">
            {activeTab === "headers" && (
              <textarea
                className="editor-area"
                placeholder={"Content-Type: application/json\nAuthorization: Bearer token"}
                value={headersText}
                onChange={(e) => setHeadersText(e.target.value)}
              />
            )}
            {activeTab === "body" && (
              <textarea
                className="editor-area"
                placeholder={'{\n  "key": "value"\n}'}
                value={bodyText}
                onChange={(e) => setBodyText(e.target.value)}
              />
            )}
          </div>
        </div>
      </main>

      {/* Right Panel: Response Viewer */}
      <section className="response-viewer">
        <div className="panel-header">
          <h2>Response</h2>
          {response && (
            <div className="response-meta">
              <span
                className="status-badge"
                style={{ color: statusColor(response.status) }}
              >
                {response.status} {response.status_text}
              </span>
              <span className="meta-item">{response.elapsed_ms}ms</span>
              <span className="meta-item">{response.size_bytes}B</span>
            </div>
          )}
        </div>

        {error && (
          <div className="panel-body">
            <div className="error-message">{error}</div>
          </div>
        )}

        {response && (
          <>
            <div className="tabs">
              <button
                className={`tab ${responseTab === "body" ? "active" : ""}`}
                onClick={() => setResponseTab("body")}
              >
                Body
              </button>
              <button
                className={`tab ${responseTab === "headers" ? "active" : ""}`}
                onClick={() => setResponseTab("headers")}
              >
                Headers
              </button>
            </div>
            <div className="panel-body">
              {responseTab === "body" && (
                <pre className="response-body">
                  {formatBody(response.body)}
                </pre>
              )}
              {responseTab === "headers" && (
                <div className="response-headers">
                  {Object.entries(response.headers).map(([key, value]) => (
                    <div key={key} className="header-row">
                      <span className="header-key">{key}</span>
                      <span className="header-value">{value}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </>
        )}

        {!response && !error && (
          <div className="panel-body">
            <p className="placeholder">Send a request to see the response</p>
          </div>
        )}
      </section>
    </div>
  );
}

export default App;
