/**
 * Variable interpolation engine — TypeScript port of wire-core's VariableScope.
 *
 * Replaces {{var}} placeholders from a layered scope where the last-added layer
 * has the highest priority (searched first).
 */

/** Regex matching {{var}}, {{ var }}, {{my-var}}, {{my.var}} */
const VAR_PATTERN = /\{\{(\s*[\w.-]+\s*)\}\}/g;

/**
 * Layered variable scope. Push layers in order of ascending priority:
 * global → environment → collection → request → chain extractions.
 *
 * Resolve searches layers in reverse (last added = highest priority).
 */
export class VariableScope {
  private layers: Map<string, string>[] = [];

  /** Push a new layer of variables (higher priority than previous layers). */
  pushLayer(vars: Record<string, string>): void {
    this.layers.push(new Map(Object.entries(vars)));
  }

  /** Resolve a variable name by searching layers from top (highest) to bottom. */
  resolve(name: string): string | undefined {
    for (let i = this.layers.length - 1; i >= 0; i--) {
      const value = this.layers[i].get(name);
      if (value !== undefined) return value;
    }
    return undefined;
  }

  /** Get all resolved variables (later layers override earlier ones). */
  resolvedMap(): Record<string, string> {
    const result: Record<string, string> = {};
    for (const layer of this.layers) {
      for (const [k, v] of layer) {
        result[k] = v;
      }
    }
    return result;
  }

  /** Get all variable names available across all layers. */
  allNames(): string[] {
    const names = new Set<string>();
    for (const layer of this.layers) {
      for (const key of layer.keys()) {
        names.add(key);
      }
    }
    return [...names].sort();
  }
}

/**
 * Interpolate {{var}} placeholders in a string using the given scope.
 * Throws VariableNotFoundError on first unresolved variable.
 *
 * Note: This does NOT resolve secrets ($env:, $dotenv:, etc.). The extension
 * leaves secret values as-is — the caller should check for secrets separately
 * and fall back to the CLI for requests that contain them.
 */
export function interpolate(template: string, scope: VariableScope): string {
  // Reset regex state (global flag means it retains lastIndex)
  VAR_PATTERN.lastIndex = 0;

  return template.replace(VAR_PATTERN, (fullMatch, captured: string) => {
    const varName = captured.trim();
    const value = scope.resolve(varName);
    if (value === undefined) {
      throw new VariableNotFoundError(varName);
    }
    return value;
  });
}

/**
 * Interpolate all values in a string→string map.
 * Keys are left unchanged; only values are interpolated.
 */
export function interpolateMap(
  map: Record<string, string>,
  scope: VariableScope
): Record<string, string> {
  const result: Record<string, string> = {};
  for (const [key, value] of Object.entries(map)) {
    result[key] = interpolate(value, scope);
  }
  return result;
}

/**
 * Interpolate variables in the URL, including query params embedded in the URL string.
 */
export function interpolateUrl(url: string, scope: VariableScope): string {
  return interpolate(url, scope);
}

/**
 * Check if a string contains any {{var}} placeholders.
 */
export function hasVariables(text: string): boolean {
  VAR_PATTERN.lastIndex = 0;
  return VAR_PATTERN.test(text);
}

/**
 * Extract all variable names referenced in a string (e.g., for autocomplete).
 */
export function extractVariableNames(text: string): string[] {
  VAR_PATTERN.lastIndex = 0;
  const names: string[] = [];
  let match: RegExpExecArray | null;
  while ((match = VAR_PATTERN.exec(text)) !== null) {
    names.push(match[1].trim());
  }
  return names;
}

export class VariableNotFoundError extends Error {
  constructor(public readonly variableName: string) {
    super(`Variable not found: ${variableName}`);
    this.name = 'VariableNotFoundError';
  }
}
