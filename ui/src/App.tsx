import { useState } from "react";
import "./App.css";

function App() {
  const [method, setMethod] = useState("GET");
  const [url, setUrl] = useState("");

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
          />
          <button className="send-btn">Send</button>
        </div>
        <div className="request-tabs">
          <div className="tabs">
            <button className="tab active">Params</button>
            <button className="tab">Headers</button>
            <button className="tab">Body</button>
            <button className="tab">Auth</button>
          </div>
          <div className="tab-content">
            <p className="placeholder">Select a tab to configure request</p>
          </div>
        </div>
      </main>

      {/* Right Panel: Response Viewer */}
      <section className="response-viewer">
        <div className="panel-header">
          <h2>Response</h2>
        </div>
        <div className="panel-body">
          <p className="placeholder">Send a request to see the response</p>
        </div>
      </section>
    </div>
  );
}

export default App;
