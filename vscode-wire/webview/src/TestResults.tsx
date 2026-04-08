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

interface Props {
  results: TestRunSummary;
}

export function TestResults({ results }: Props) {
  const allPassed = results.failed === 0;

  return (
    <div style={{
      borderTop: '1px solid var(--vscode-widget-border)',
      padding: 12,
    }}>
      {/* Summary bar */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        marginBottom: 8,
      }}>
        <span style={{
          fontWeight: 700,
          color: allPassed
            ? 'var(--vscode-testing-iconPassed, #4ec9b0)'
            : 'var(--vscode-testing-iconFailed, #f44747)',
          fontSize: 13,
        }}>
          {allPassed ? 'All tests passed' : `${results.failed} failed`}
        </span>
        <span style={{ color: 'var(--vscode-descriptionForeground)', fontSize: 12 }}>
          {results.passed}/{results.total_assertions} assertions
        </span>
      </div>

      {/* Per-request results */}
      {results.results.map((req, ri) => (
        <div key={ri} style={{ marginBottom: 8 }}>
          {results.results.length > 1 && (
            <div style={{
              fontSize: 12,
              color: 'var(--vscode-descriptionForeground)',
              marginBottom: 4,
            }}>
              {req.name}
            </div>
          )}

          {req.error && (
            <div style={{
              padding: '4px 8px',
              background: 'var(--vscode-inputValidation-errorBackground)',
              color: 'var(--vscode-errorForeground)',
              fontSize: 12,
              borderRadius: 2,
              marginBottom: 4,
            }}>
              {req.error}
            </div>
          )}

          {req.assertions.map((a, ai) => (
            <div key={ai} style={{
              display: 'flex',
              alignItems: 'baseline',
              gap: 8,
              padding: '2px 0',
              fontFamily: 'var(--vscode-editor-font-family, monospace)',
              fontSize: 13,
            }}>
              <span style={{
                color: a.passed
                  ? 'var(--vscode-testing-iconPassed, #4ec9b0)'
                  : 'var(--vscode-testing-iconFailed, #f44747)',
              }}>
                {a.passed ? '\u2713' : '\u2717'}
              </span>
              <span style={{ color: 'var(--vscode-symbolIcon-fieldForeground, #9cdcfe)' }}>
                {a.field}
              </span>
              <span style={{ color: 'var(--vscode-descriptionForeground)' }}>
                {a.operator}
              </span>
              <span style={{ color: a.passed ? 'var(--vscode-charts-green, #4ec9b0)' : 'var(--vscode-charts-red, #f44747)' }}>
                {a.expected}
              </span>
              {!a.passed && (
                <span style={{ color: 'var(--vscode-descriptionForeground)', fontSize: 12 }}>
                  (got: {a.actual})
                </span>
              )}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}
