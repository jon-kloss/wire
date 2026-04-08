import { useState } from 'react';

interface WireResponse {
  status: number;
  status_text: string;
  headers: Record<string, string>;
  body: string;
  elapsed_ms: number;
  size_bytes: number;
}

interface Props {
  response: WireResponse;
}

type ViewMode = 'pretty' | 'raw' | 'headers';

export function ResponseViewer({ response }: Props) {
  const [viewMode, setViewMode] = useState<ViewMode>('pretty');

  const statusColor = response.status < 300
    ? 'var(--vscode-testing-iconPassed, #4ec9b0)'
    : response.status < 400
    ? 'var(--vscode-editorWarning-foreground, #dcdcaa)'
    : 'var(--vscode-testing-iconFailed, #f44747)';

  // Try to parse body as JSON for pretty view
  let parsedBody: unknown = null;
  let isJson = false;
  try {
    parsedBody = JSON.parse(response.body);
    isJson = true;
  } catch {
    // Not JSON
  }

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes}B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
  };

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
      {/* Status bar */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        padding: '8px 12px',
        borderBottom: '1px solid var(--vscode-widget-border)',
      }}>
        <span style={{
          fontWeight: 700,
          color: statusColor,
          fontSize: 14,
        }}>
          {response.status} {response.status_text}
        </span>
        <span style={{ color: 'var(--vscode-descriptionForeground)', fontSize: 12 }}>
          {response.elapsed_ms}ms
        </span>
        <span style={{ color: 'var(--vscode-descriptionForeground)', fontSize: 12 }}>
          {formatSize(response.size_bytes)}
        </span>
        <div style={{ flex: 1 }} />
        {/* View mode toggle */}
        {(['pretty', 'raw', 'headers'] as const).map((mode) => (
          <button
            key={mode}
            onClick={() => setViewMode(mode)}
            style={{
              background: viewMode === mode ? 'var(--vscode-button-background)' : 'transparent',
              color: viewMode === mode ? 'var(--vscode-button-foreground)' : 'var(--vscode-descriptionForeground)',
              border: 'none',
              padding: '2px 8px',
              cursor: 'pointer',
              fontSize: 12,
              borderRadius: 2,
              textTransform: 'capitalize',
            }}
          >
            {mode}
          </button>
        ))}
      </div>

      {/* Body */}
      <div style={{ flex: 1, overflow: 'auto', padding: 12 }}>
        {viewMode === 'pretty' && isJson && (
          <JsonTree data={parsedBody} />
        )}
        {viewMode === 'pretty' && !isJson && (
          <pre style={{
            fontFamily: 'var(--vscode-editor-font-family, monospace)',
            fontSize: 13,
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
          }}>
            {response.body}
          </pre>
        )}
        {viewMode === 'raw' && (
          <pre style={{
            fontFamily: 'var(--vscode-editor-font-family, monospace)',
            fontSize: 13,
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
          }}>
            {isJson ? JSON.stringify(parsedBody, null, 2) : response.body}
          </pre>
        )}
        {viewMode === 'headers' && (
          <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
            <tbody>
              {Object.entries(response.headers).map(([key, value]) => (
                <tr key={key}>
                  <td style={{
                    padding: '2px 8px',
                    fontFamily: 'var(--vscode-editor-font-family, monospace)',
                    fontWeight: 600,
                    verticalAlign: 'top',
                    whiteSpace: 'nowrap',
                  }}>
                    {key}
                  </td>
                  <td style={{
                    padding: '2px 8px',
                    fontFamily: 'var(--vscode-editor-font-family, monospace)',
                    color: 'var(--vscode-descriptionForeground)',
                    wordBreak: 'break-all',
                  }}>
                    {value}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

/** Recursive collapsible JSON tree viewer */
function JsonTree({ data, depth = 0 }: { data: unknown; depth?: number }) {
  if (data === null) return <span style={{ color: 'var(--vscode-debugTokenExpression-error, #f44747)' }}>null</span>;
  if (typeof data === 'boolean') return <span style={{ color: 'var(--vscode-debugTokenExpression-boolean, #569cd6)' }}>{String(data)}</span>;
  if (typeof data === 'number') return <span style={{ color: 'var(--vscode-debugTokenExpression-number, #b5cea8)' }}>{data}</span>;
  if (typeof data === 'string') return <span style={{ color: 'var(--vscode-debugTokenExpression-string, #ce9178)' }}>"{data}"</span>;

  if (Array.isArray(data)) {
    if (data.length === 0) return <span>[]</span>;
    return <CollapsibleNode label={`Array(${data.length})`} depth={depth}>
      {data.map((item, i) => (
        <div key={i} style={{ paddingLeft: 16 }}>
          <span style={{ color: 'var(--vscode-descriptionForeground)' }}>{i}: </span>
          <JsonTree data={item} depth={depth + 1} />
        </div>
      ))}
    </CollapsibleNode>;
  }

  if (typeof data === 'object') {
    const entries = Object.entries(data as Record<string, unknown>);
    if (entries.length === 0) return <span>{'{}'}</span>;
    return <CollapsibleNode label={`Object(${entries.length})`} depth={depth}>
      {entries.map(([key, value]) => (
        <div key={key} style={{ paddingLeft: 16 }}>
          <span style={{ color: 'var(--vscode-symbolIcon-fieldForeground, #9cdcfe)' }}>{key}</span>
          <span style={{ color: 'var(--vscode-descriptionForeground)' }}>: </span>
          <JsonTree data={value} depth={depth + 1} />
        </div>
      ))}
    </CollapsibleNode>;
  }

  return <span>{String(data)}</span>;
}

function CollapsibleNode({ label, children, depth }: { label: string; children: React.ReactNode; depth: number }) {
  const [collapsed, setCollapsed] = useState(depth > 2);

  return (
    <span>
      <span
        onClick={() => setCollapsed(!collapsed)}
        style={{ cursor: 'pointer', userSelect: 'none', color: 'var(--vscode-descriptionForeground)' }}
      >
        {collapsed ? '▶ ' : '▼ '}{label}
      </span>
      {!collapsed && <div>{children}</div>}
    </span>
  );
}
