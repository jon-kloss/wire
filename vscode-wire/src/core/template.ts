/**
 * Template resolution — TypeScript port of wire-core's template merging.
 *
 * Resolves the `extends` field in WireRequest files by loading parent templates
 * and merging fields according to Wire's rules:
 *   - Headers: additive (request wins on conflict)
 *   - Params: additive (request wins on conflict)
 *   - Body: top-level JSON object merge when both are same type; override otherwise
 *   - Tests: additive (base + request)
 *   - Response schema: additive, deduplicated
 *   - URL/Method/Name: override (request wins if non-empty)
 *   - Chain: override (request wins if non-empty)
 *   - Snapshot: override (request wins if present)
 */

import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { existsSync } from 'node:fs';
import * as yaml from 'js-yaml';
import type { WireRequest, Body } from './types.js';
import { parseRequest } from './yaml.js';

const MAX_TEMPLATE_DEPTH = 3;

/**
 * Resolve a request's template chain.
 * If the request has `extends`, loads the template and merges recursively.
 * If no `extends`, applies default_templates from the collection (if any).
 */
export async function resolveTemplate(
  request: WireRequest,
  wireDir: string,
  defaultTemplates: string[] = [],
): Promise<WireRequest> {
  if (request.extends) {
    // Explicit extends takes full priority
    return resolveTemplateInner(request, wireDir, [], 0);
  }

  if (defaultTemplates.length === 0) {
    return request;
  }

  // Apply all defaults additively, then request on top
  let base = emptyRequest();

  for (const tmplName of defaultTemplates) {
    const template = await loadTemplate(tmplName, wireDir);
    const resolved = await resolveTemplateInner(template, wireDir, [], 0);
    base = mergeRequests(base, resolved);
  }

  const merged = mergeRequests(base, request);
  merged.extends = defaultTemplates.join(', ');
  return merged;
}

/**
 * Recursively resolve template inheritance.
 * Tracks the chain for circular dependency detection.
 */
async function resolveTemplateInner(
  request: WireRequest,
  wireDir: string,
  chain: string[],
  depth: number,
): Promise<WireRequest> {
  const templateName = request.extends;
  if (!templateName) {
    return request; // Base case: no extends
  }

  // Circular dependency detection
  if (chain.includes(templateName)) {
    chain.push(templateName);
    throw new TemplateError(
      `Circular template dependency: ${chain.join(' -> ')}`
    );
  }

  chain.push(templateName);

  // Depth check
  if (depth >= MAX_TEMPLATE_DEPTH) {
    throw new TemplateError(
      `Template inheritance too deep (max ${MAX_TEMPLATE_DEPTH}): ${chain.join(' -> ')}`
    );
  }

  // Load parent template and resolve its chain recursively
  const template = await loadTemplate(templateName, wireDir);
  const resolvedTemplate = await resolveTemplateInner(
    template,
    wireDir,
    chain,
    depth + 1,
  );

  // Merge: resolved parent + current request (request wins)
  const merged = mergeRequests(resolvedTemplate, request);
  merged.extends = templateName; // Preserve for GUI display
  return merged;
}

/**
 * Load a template file from the .wire/templates/ directory.
 */
async function loadTemplate(name: string, wireDir: string): Promise<WireRequest> {
  // Validate template name (prevent path traversal)
  if (name.includes('/') || name.includes('\\') || name.includes('..')) {
    throw new TemplateError(
      `Invalid template name: ${name} (must not contain path separators or '..')`
    );
  }

  const templatePath = join(wireDir, 'templates', `${name}.wire.yaml`);

  if (!existsSync(templatePath)) {
    throw new TemplateError(
      `Template not found: ${name} (expected at ${templatePath})`
    );
  }

  const content = await readFile(templatePath, 'utf-8');
  return parseRequest(content, templatePath);
}

/**
 * Merge two requests according to Wire's rules.
 * `over` (the child/request) wins on conflicts.
 */
function mergeRequests(base: WireRequest, over: WireRequest): WireRequest {
  // Headers: additive (over wins on key conflict)
  const headers = { ...base.headers, ...over.headers };

  // Params: additive (over wins on key conflict)
  const params = { ...base.params, ...over.params };

  // Body: override or top-level JSON merge
  const body = mergeBody(base.body, over.body);

  // Tests: additive (base + over)
  const tests = [...base.tests, ...over.tests];

  // Response schema: additive, deduplicated
  const schemaSet = new Set(base.response_schema.map((e) => JSON.stringify(e)));
  const responseSchema = [...base.response_schema];
  for (const entry of over.response_schema) {
    const key = JSON.stringify(entry);
    if (!schemaSet.has(key)) {
      responseSchema.push(entry);
      schemaSet.add(key);
    }
  }

  return {
    name: over.name,
    method: over.method || base.method,
    url: over.url || base.url,
    headers,
    params,
    body,
    extends: undefined, // Will be set by caller if needed
    tests,
    response_schema: responseSchema,
    chain: over.chain.length > 0 ? over.chain : base.chain,
    snapshot: over.snapshot ?? base.snapshot,
  };
}

/**
 * Merge body fields according to Wire's rules:
 * - If both are JSON objects of the same type, do top-level merge (over wins on conflict)
 * - Otherwise, over wins entirely
 */
function mergeBody(base: Body | undefined, over: Body | undefined): Body | undefined {
  if (over) {
    if (base && base.type === over.type) {
      // Both same type — check if both are JSON objects for top-level merge
      if (
        typeof base.content === 'object' &&
        base.content !== null &&
        !Array.isArray(base.content) &&
        typeof over.content === 'object' &&
        over.content !== null &&
        !Array.isArray(over.content)
      ) {
        return {
          type: over.type,
          content: {
            ...(base.content as Record<string, unknown>),
            ...(over.content as Record<string, unknown>),
          },
        };
      }
    }
    // Type mismatch or non-object: over wins entirely
    return over;
  }
  return base;
}

/** Create an empty request (used as base for default template accumulation). */
function emptyRequest(): WireRequest {
  return {
    name: '',
    method: '',
    url: '',
    headers: {},
    params: {},
    body: undefined,
    extends: undefined,
    tests: [],
    response_schema: [],
    chain: [],
    snapshot: undefined,
  };
}

/**
 * Get the resolved view of a request's headers, showing which came from templates
 * and which are from the request itself. Useful for the template manager UI.
 */
export async function getHeaderSources(
  request: WireRequest,
  wireDir: string,
  defaultTemplates: string[] = [],
): Promise<Array<{ key: string; value: string; source: 'template' | 'request' }>> {
  // Load the template base (without the request's own headers)
  let templateHeaders: Record<string, string> = {};

  if (request.extends) {
    try {
      const template = await loadTemplate(request.extends, wireDir);
      const resolved = await resolveTemplateInner(template, wireDir, [], 0);
      templateHeaders = resolved.headers;
    } catch {
      // Template not found — all headers are from request
    }
  } else if (defaultTemplates.length > 0) {
    for (const tmplName of defaultTemplates) {
      try {
        const template = await loadTemplate(tmplName, wireDir);
        const resolved = await resolveTemplateInner(template, wireDir, [], 0);
        templateHeaders = { ...templateHeaders, ...resolved.headers };
      } catch {
        // Skip missing defaults
      }
    }
  }

  // Build the result: template headers first, then request overrides/additions
  const result: Array<{ key: string; value: string; source: 'template' | 'request' }> = [];
  const allKeys = new Set([...Object.keys(templateHeaders), ...Object.keys(request.headers)]);

  for (const key of allKeys) {
    if (key in request.headers) {
      result.push({
        key,
        value: request.headers[key],
        source: key in templateHeaders ? 'request' : 'request', // Override or new
      });
    } else {
      result.push({
        key,
        value: templateHeaders[key],
        source: 'template',
      });
    }
  }

  return result;
}

export class TemplateError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'TemplateError';
  }
}
