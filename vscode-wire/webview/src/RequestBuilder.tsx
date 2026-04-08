import { useState } from 'react';

interface WireRequest {
  name: string;
  method: string;
  url: string;
  headers: Record<string, string>;
  params: Record<string, string>;
  body?: { type: string; content: unknown };
  extends?: string;
  tests: Array<{ field: string; [key: string]: unknown }>;
  chain: unknown[];
}

interface Props {
  request: WireRequest;
  onSend: () => void;
  onTest: () => void;
  status: 'idle' | 'sending' | 'testing';
}

const METHOD_COLORS: Record<string, string> = {
  GET: 'var(--vscode-charts-green, #4ec9b0)',
  POST: 'var(--vscode-charts-yellow, #dcdcaa)',
  PUT: 'var(--vscode-charts-blue, #569cd6)',
  PATCH: 'var(--vscode-charts-purple, #c586c0)',
  DELETE: 'var(--vscode-charts-red, #f44747)',
};

const TABS = ['Headers', 'Params', 'Body', 'Tests'] as const;
type Tab = typeof TABS[number];

export function RequestBuilder({ request, onSend, onTest, status }: Props) {
  const [activeTab, setActiveTab] = useState<Tab>('Headers');

  const headerEntries = Object.entries(request.headers);
  const paramEntries = Object.entries(request.params);
  const methodColor = METHOD_COLORS[request.method] ?? 'var(--vscode-foreground)';

  return (
    <div style={{ padding: 12, borderBottom: '1px solid var(--vscode-widget-border)' }}>
      {/* Method + URL + Send */}
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 8 }}>
        <span style={{
          fontWeight: 700,
          color: methodColor,
          minWidth: 60,
          fontSize: 13,
        }}>
          {request.method}
        </span>
        <input
          type="text"
          value={request.url}
          readOnly
          style={{
            flex: 1,
            background: 'var(--vscode-input-background)',
            color: 'var(--vscode-input-foreground)',
            border: '1px solid var(--vscode-input-border)',
            padding: '4px 8px',
            fontFamily: 'var(--vscode-editor-font-family, monospace)',
            fontSize: 13,
            borderRadius: 2,
          }}
        />
        <button
          onClick={onSend}
          disabled={status !== 'idle'}
          style={{
            background: 'var(--vscode-button-background)',
            color: 'var(--vscode-button-foreground)',
            border: 'none',
            padding: '6px 16px',
            cursor: status === 'idle' ? 'pointer' : 'wait',
            borderRadius: 2,
            fontWeight: 600,
            fontSize: 13,
            opacity: status !== 'idle' ? 0.6 : 1,
          }}
        >
          {status === 'sending' ? 'Sending...' : 'Send'}
        </button>
        <button
          onClick={onTest}
          disabled={status !== 'idle' || request.tests.length === 0}
          style={{
            background: 'var(--vscode-button-secondaryBackground)',
            color: 'var(--vscode-button-secondaryForeground)',
            border: 'none',
            padding: '6px 12px',
            cursor: status === 'idle' && request.tests.length > 0 ? 'pointer' : 'default',
            borderRadius: 2,
            fontSize: 13,
            opacity: status !== 'idle' || request.tests.length === 0 ? 0.5 : 1,
          }}
        >
          {status === 'testing' ? 'Testing...' : `Test (${request.tests.length})`}
        </button>
      </div>

      {/* Template badge */}
      {request.extends && (
        <div style={{
          fontSize: 12,
          color: 'var(--vscode-descriptionForeground)',
          marginBottom: 8,
        }}>
          extends: <span style={{ color: 'var(--vscode-textLink-foreground)' }}>{request.extends}</span>
        </div>
      )}

      {/* Tabs */}
      <div style={{ display: 'flex', gap: 0, borderBottom: '1px solid var(--vscode-widget-border)' }}>
        {TABS.map((tab) => {
          const count = tab === 'Headers' ? headerEntries.length
            : tab === 'Params' ? paramEntries.length
            : tab === 'Tests' ? request.tests.length
            : request.body ? 1 : 0;
          return (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              style={{
                background: activeTab === tab
                  ? 'var(--vscode-tab-activeBackground, transparent)'
                  : 'transparent',
                color: activeTab === tab
                  ? 'var(--vscode-foreground)'
                  : 'var(--vscode-descriptionForeground)',
                border: 'none',
                borderBottom: activeTab === tab ? '2px solid var(--vscode-focusBorder)' : '2px solid transparent',
                padding: '6px 12px',
                cursor: 'pointer',
                fontSize: 13,
              }}
            >
              {tab}{count > 0 ? ` (${count})` : ''}
            </button>
          );
        })}
      </div>

      {/* Tab content */}
      <div style={{ padding: '8px 0', maxHeight: 200, overflowY: 'auto' }}>
        {activeTab === 'Headers' && <KeyValueTable entries={headerEntries} emptyText="No headers" />}
        {activeTab === 'Params' && <KeyValueTable entries={paramEntries} emptyText="No query params" />}
        {activeTab === 'Body' && <BodyView body={request.body} />}
        {activeTab === 'Tests' && <TestList tests={request.tests} />}
      </div>
    </div>
  );
}

function KeyValueTable({ entries, emptyText }: { entries: [string, string][]; emptyText: string }) {
  if (entries.length === 0) {
    return <p style={{ color: 'var(--vscode-descriptionForeground)', fontSize: 12, padding: 4 }}>{emptyText}</p>;
  }
  return (
    <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
      <thead>
        <tr>
          <th style={{ textAlign: 'left', padding: '2px 8px', color: 'var(--vscode-descriptionForeground)', fontWeight: 600 }}>Key</th>
          <th style={{ textAlign: 'left', padding: '2px 8px', color: 'var(--vscode-descriptionForeground)', fontWeight: 600 }}>Value</th>
        </tr>
      </thead>
      <tbody>
        {entries.map(([key, value]) => (
          <tr key={key}>
            <td style={{ padding: '2px 8px', fontFamily: 'var(--vscode-editor-font-family, monospace)' }}>{key}</td>
            <td style={{ padding: '2px 8px', fontFamily: 'var(--vscode-editor-font-family, monospace)', color: 'var(--vscode-descriptionForeground)' }}>{value}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function BodyView({ body }: { body?: { type: string; content: unknown } }) {
  if (!body) {
    return <p style={{ color: 'var(--vscode-descriptionForeground)', fontSize: 12, padding: 4 }}>No body</p>;
  }
  const content = typeof body.content === 'string'
    ? body.content
    : JSON.stringify(body.content, null, 2);

  return (
    <div>
      <span style={{
        fontSize: 11,
        padding: '2px 6px',
        background: 'var(--vscode-badge-background)',
        color: 'var(--vscode-badge-foreground)',
        borderRadius: 2,
        marginBottom: 4,
        display: 'inline-block',
      }}>
        {body.type}
      </span>
      <pre style={{
        marginTop: 4,
        padding: 8,
        background: 'var(--vscode-textCodeBlock-background)',
        borderRadius: 2,
        fontSize: 13,
        fontFamily: 'var(--vscode-editor-font-family, monospace)',
        whiteSpace: 'pre-wrap',
        wordBreak: 'break-word',
        maxHeight: 150,
        overflowY: 'auto',
      }}>
        {content}
      </pre>
    </div>
  );
}

function TestList({ tests }: { tests: Array<{ field: string; [key: string]: unknown }> }) {
  if (tests.length === 0) {
    return <p style={{ color: 'var(--vscode-descriptionForeground)', fontSize: 12, padding: 4 }}>No test assertions</p>;
  }
  return (
    <div style={{ fontSize: 13 }}>
      {tests.map((test, i) => {
        const operator = Object.keys(test).find(k => k !== 'field') ?? '?';
        const value = test[operator];
        return (
          <div key={i} style={{
            padding: '2px 4px',
            fontFamily: 'var(--vscode-editor-font-family, monospace)',
          }}>
            <span style={{ color: 'var(--vscode-symbolIcon-fieldForeground, #9cdcfe)' }}>{test.field}</span>
            {' '}
            <span style={{ color: 'var(--vscode-descriptionForeground)' }}>{operator}</span>
            {' '}
            <span style={{ color: 'var(--vscode-charts-green, #4ec9b0)' }}>{JSON.stringify(value)}</span>
          </div>
        );
      })}
    </div>
  );
}
