import { useState, useEffect, useCallback } from 'react';
import { RequestBuilder } from './RequestBuilder';
import { ResponseViewer } from './ResponseViewer';
import { TestResults } from './TestResults';

const vscode = acquireVsCodeApi();

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

interface WireResponse {
  status: number;
  status_text: string;
  headers: Record<string, string>;
  body: string;
  elapsed_ms: number;
  size_bytes: number;
}

interface TestRunSummary {
  results: Array<{
    file: string;
    name: string;
    assertions: Array<{
      field: string;
      operator: string;
      passed: boolean;
      expected: string;
      actual: string;
    }>;
    error?: string;
  }>;
  total_assertions: number;
  passed: number;
  failed: number;
}

type AppStatus = 'idle' | 'sending' | 'testing';

export function App() {
  const [request, setRequest] = useState<WireRequest | null>(null);
  const [response, setResponse] = useState<WireResponse | null>(null);
  const [testResults, setTestResults] = useState<TestRunSummary | null>(null);
  const [status, setStatus] = useState<AppStatus>('idle');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const handler = (event: MessageEvent) => {
      const msg = event.data;
      switch (msg.type) {
        case 'loadRequest':
          setRequest(msg.request);
          setResponse(null);
          setTestResults(null);
          setError(null);
          break;
        case 'sending':
          setStatus('sending');
          setError(null);
          break;
        case 'testing':
          setStatus('testing');
          setError(null);
          break;
        case 'response':
          setResponse(msg.response);
          setStatus('idle');
          break;
        case 'testResults':
          setTestResults(msg.results);
          setStatus('idle');
          break;
        case 'error':
          setError(msg.message);
          setStatus('idle');
          break;
      }
    };
    window.addEventListener('message', handler);
    return () => window.removeEventListener('message', handler);
  }, []);

  const handleSend = useCallback(() => {
    vscode.postMessage({ type: 'send' });
  }, []);

  const handleTest = useCallback(() => {
    vscode.postMessage({ type: 'test' });
  }, []);

  if (!request) {
    return (
      <div className="wire-app" style={{ padding: 20 }}>
        <p style={{ color: 'var(--vscode-descriptionForeground)' }}>Loading request...</p>
      </div>
    );
  }

  return (
    <div className="wire-app" style={{ display: 'flex', flexDirection: 'column', height: '100vh' }}>
      <RequestBuilder
        request={request}
        onSend={handleSend}
        onTest={handleTest}
        status={status}
      />
      {error && (
        <div style={{
          padding: '8px 12px',
          background: 'var(--vscode-inputValidation-errorBackground)',
          border: '1px solid var(--vscode-inputValidation-errorBorder)',
          color: 'var(--vscode-errorForeground)',
          margin: '0 12px',
        }}>
          {error}
        </div>
      )}
      {response && <ResponseViewer response={response} />}
      {testResults && <TestResults results={testResults} />}
    </div>
  );
}

declare function acquireVsCodeApi(): {
  postMessage(message: unknown): void;
  getState(): unknown;
  setState(state: unknown): void;
};
