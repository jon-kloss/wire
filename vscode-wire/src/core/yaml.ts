/**
 * YAML parser for .wire.yaml request files.
 * Parses raw YAML into typed WireRequest, WireCollection, and Environment objects.
 */

import { readFile } from 'node:fs/promises';
import * as yaml from 'js-yaml';
import type {
  WireRequest,
  WireCollection,
  Environment,
  Body,
  BodyType,
  Assertion,
  ChainStep,
  SnapshotConfig,
} from './types.js';

// ─── Raw YAML shapes (before normalization) ─────────────────────────────────

/** Raw shape as it comes out of js-yaml — fields may be missing */
interface RawRequest {
  name?: string;
  method?: string;
  url?: string;
  headers?: Record<string, string>;
  params?: Record<string, string>;
  body?: RawBody;
  extends?: string;
  tests?: RawAssertion[];
  response_schema?: Array<[string, string]> | Record<string, string>;
  chain?: RawChainStep[];
  snapshot?: RawSnapshotConfig;
}

interface RawBody {
  type?: string;
  content?: unknown;
}

interface RawAssertion {
  field?: string;
  equals?: unknown;
  not_equals?: unknown;
  contains?: string;
  starts_with?: string;
  ends_with?: string;
  less_than?: number;
  greater_than?: number;
  is_array?: boolean;
  is_object?: boolean;
  is_string?: boolean;
  is_number?: boolean;
  exists?: boolean;
  body_contains?: string;
  body_matches?: string;
}

interface RawChainStep {
  run?: string;
  extract?: Record<string, string>;
  persist?: boolean;
}

interface RawSnapshotConfig {
  ignore?: string[];
}

interface RawCollection {
  name?: string;
  version?: number;
  active_env?: string;
  default_template?: string;
  default_templates?: string[];
  source_dir?: string;
}

interface RawEnvironment {
  name?: string;
  variables?: Record<string, string>;
}

// ─── Parsing functions ───────────────────────────────────────────────────────

/** Parse a .wire.yaml request file from disk */
export async function parseRequestFile(filePath: string): Promise<WireRequest> {
  const content = await readFile(filePath, 'utf-8');
  return parseRequest(content, filePath);
}

/** Parse a .wire.yaml request from a YAML string */
export function parseRequest(yamlContent: string, source?: string): WireRequest {
  const raw = yaml.load(yamlContent) as RawRequest | null;

  if (!raw || typeof raw !== 'object') {
    throw new ParseError('Empty or invalid YAML', source);
  }

  if (!raw.name) throw new ParseError('Missing required field: name', source);
  if (!raw.method) throw new ParseError('Missing required field: method', source);
  if (!raw.url) throw new ParseError('Missing required field: url', source);

  return {
    name: raw.name,
    method: raw.method.toUpperCase(),
    url: raw.url,
    headers: raw.headers ?? {},
    params: raw.params ?? {},
    body: raw.body ? normalizeBody(raw.body) : undefined,
    extends: raw.extends,
    tests: (raw.tests ?? []).map(normalizeAssertion),
    response_schema: normalizeResponseSchema(raw.response_schema),
    chain: (raw.chain ?? []).map(normalizeChainStep),
    snapshot: raw.snapshot ? normalizeSnapshotConfig(raw.snapshot) : undefined,
  };
}

/** Parse a wire.yaml collection metadata file from disk */
export async function parseCollectionFile(filePath: string): Promise<WireCollection> {
  const content = await readFile(filePath, 'utf-8');
  return parseCollection(content, filePath);
}

/** Parse a wire.yaml collection from a YAML string */
export function parseCollection(yamlContent: string, source?: string): WireCollection {
  const raw = yaml.load(yamlContent) as RawCollection | null;

  if (!raw || typeof raw !== 'object') {
    throw new ParseError('Empty or invalid wire.yaml', source);
  }

  return {
    name: raw.name ?? 'Unnamed Collection',
    version: raw.version ?? 1,
    active_env: raw.active_env,
    default_template: raw.default_template,
    default_templates: raw.default_templates ?? (raw.default_template ? [raw.default_template] : []),
    source_dir: raw.source_dir,
  };
}

/** Parse an environment file from disk */
export async function parseEnvironmentFile(filePath: string): Promise<Environment> {
  const content = await readFile(filePath, 'utf-8');
  return parseEnvironment(content, filePath);
}

/** Parse an environment from a YAML string */
export function parseEnvironment(yamlContent: string, source?: string): Environment {
  const raw = yaml.load(yamlContent) as RawEnvironment | null;

  if (!raw || typeof raw !== 'object') {
    throw new ParseError('Empty or invalid environment file', source);
  }

  return {
    name: raw.name ?? 'unnamed',
    variables: raw.variables ?? {},
  };
}

// ─── Normalization helpers ───────────────────────────────────────────────────

function normalizeBody(raw: RawBody): Body {
  const bodyType = (raw.type ?? 'json').toLowerCase() as BodyType;
  if (!['json', 'text', 'formdata'].includes(bodyType)) {
    throw new ParseError(`Invalid body type: ${raw.type}`);
  }
  return {
    type: bodyType,
    content: raw.content ?? (bodyType === 'json' ? {} : ''),
  };
}

function normalizeAssertion(raw: RawAssertion): Assertion {
  if (!raw.field) {
    throw new ParseError('Test assertion missing required field: field');
  }
  return {
    field: raw.field,
    ...(raw.equals !== undefined && { equals: raw.equals }),
    ...(raw.not_equals !== undefined && { not_equals: raw.not_equals }),
    ...(raw.contains !== undefined && { contains: raw.contains }),
    ...(raw.starts_with !== undefined && { starts_with: raw.starts_with }),
    ...(raw.ends_with !== undefined && { ends_with: raw.ends_with }),
    ...(raw.less_than !== undefined && { less_than: raw.less_than }),
    ...(raw.greater_than !== undefined && { greater_than: raw.greater_than }),
    ...(raw.is_array !== undefined && { is_array: raw.is_array }),
    ...(raw.is_object !== undefined && { is_object: raw.is_object }),
    ...(raw.is_string !== undefined && { is_string: raw.is_string }),
    ...(raw.is_number !== undefined && { is_number: raw.is_number }),
    ...(raw.exists !== undefined && { exists: raw.exists }),
    ...(raw.body_contains !== undefined && { body_contains: raw.body_contains }),
    ...(raw.body_matches !== undefined && { body_matches: raw.body_matches }),
  };
}

function normalizeChainStep(raw: RawChainStep): ChainStep {
  if (!raw.run) {
    throw new ParseError('Chain step missing required field: run');
  }
  return {
    run: raw.run,
    extract: raw.extract ?? {},
    persist: raw.persist ?? false,
  };
}

function normalizeSnapshotConfig(raw: RawSnapshotConfig): SnapshotConfig {
  return {
    ignore: raw.ignore ?? [],
  };
}

function normalizeResponseSchema(
  raw?: Array<[string, string]> | Record<string, string>
): Array<[string, string]> {
  if (!raw) return [];
  if (Array.isArray(raw)) return raw;
  // Convert Record to tuples
  return Object.entries(raw);
}

// ─── Serialization (write back to YAML) ──────────────────────────────────────

/** Serialize a WireRequest back to YAML */
export function serializeRequest(request: WireRequest): string {
  const obj: Record<string, unknown> = {
    name: request.name,
    method: request.method,
    url: request.url,
  };

  if (Object.keys(request.headers).length > 0) obj.headers = request.headers;
  if (Object.keys(request.params).length > 0) obj.params = request.params;
  if (request.body) obj.body = request.body;
  if (request.extends) obj.extends = request.extends;
  if (request.tests.length > 0) obj.tests = request.tests;
  if (request.response_schema.length > 0) obj.response_schema = request.response_schema;
  if (request.chain.length > 0) obj.chain = request.chain;
  if (request.snapshot) obj.snapshot = request.snapshot;

  return yaml.dump(obj, {
    lineWidth: 120,
    noRefs: true,
    quotingType: '"',
    forceQuotes: false,
  });
}

/** Serialize a WireCollection back to YAML */
export function serializeCollection(collection: WireCollection): string {
  const obj: Record<string, unknown> = {
    name: collection.name,
    version: collection.version,
  };

  if (collection.active_env) obj.active_env = collection.active_env;
  if (collection.default_templates.length > 0) obj.default_templates = collection.default_templates;
  if (collection.source_dir) obj.source_dir = collection.source_dir;

  return yaml.dump(obj, { lineWidth: 120, noRefs: true });
}

/** Serialize an Environment back to YAML */
export function serializeEnvironment(env: Environment): string {
  return yaml.dump(
    { name: env.name, variables: env.variables },
    { lineWidth: 120, noRefs: true }
  );
}

// ─── Errors ──────────────────────────────────────────────────────────────────

export class ParseError extends Error {
  constructor(message: string, public readonly source?: string) {
    super(source ? `${message} (in ${source})` : message);
    this.name = 'ParseError';
  }
}
